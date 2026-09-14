/* v1-probe-unmeasured / SUB-PAGE TEARING probe, VERSION 2.
 *
 * Question left explicitly untested by v1-probe-generic: can a single
 * FUSE_NOTIFY_RETRIEVE reply be a *mixed-instant* image at sub-page (8-byte)
 * granularity? The earlier probe compared only the first 8 bytes of each page,
 * so it could only see tearing at 4-KiB granularity. Version 1 of this probe
 * (subpage-v1-wholesweep.c) answered nothing useful because its writer's
 * per-word store rate (~0.82 ns/word) matched the kernel's page-copy rate, so
 * writer and copier ran in lockstep and the sampled image was uniform.
 *
 * Oracle. The writer sweeps an aligned 8-byte word range in *ascending* order,
 * storing the round number r at each word with a full barrier
 * (__sync_synchronize, DMB on aarch64) between stores; one round completes
 * before the next begins. Therefore at every instant, for any two global word
 * indices i < j inside the swept range,
 *      value(word i) >= value(word j)
 * Any collected sample with v[i] < v[j] for some i < j mixes two instants of the
 * writer and is not the file's content at any instant. (An ascending copy still
 * produces such violations, because the writer's round advances between the
 * copies of two different words.)
 *
 * Geometries (the rate ratio is what decides whether tearing is observable):
 *   MODE_PAGE0  the writer sweeps ONLY the 512 words of page 0 (round = 512
 *               stores ~ 0.4-0.5 us). The daemon copies one 4-KiB page in a
 *               comparable time, so the writer's round boundary crosses the
 *               copied range in most replies. This is the decisive geometry.
 *   MODE_FILE   the writer sweeps all 16384 words (round ~ 13 us), matching the
 *               copier's whole-file rate (version 1's geometry, kept as T-SP-E).
 *   MODE_SLOW   the writer sweeps page 0 with 200 us between words: the copy of
 *               one page then cannot straddle a store at all (control).
 *
 * Tests:
 *   T-SP-A  MODE_PAGE0, retrieve ONE page (4096 B) x 200 -> sub-page tearing and
 *           its rate.
 *   T-SP-B  MODE_PAGE0, retrieve all 32 pages (128 KiB) x 20 -> sub-page tearing
 *           inside a full-size retrieve reply (the analogue of the earlier
 *           probe's T3, which only looked at word 0 of each page).
 *   T-SP-C  quiesced controls.
 *   T-SP-D  MODE_SLOW control, one page x 100.
 *   T-SP-E  MODE_FILE, one page x 200 and full reply x 20 (version 1's
 *           geometry, re-measured with the delta bug fixed).
 *   T-SP-F  MODE_PAGE0, 10 daemon-initiated writeback drains (fsync through the
 *           daemon's own fd), word-granular analysis of the delivered payload.
 *
 * Also recorded: the duration of the read(2) that delivers a retrieve reply
 * (that is where fuse_copy_pages copies the pages), so the tear rate can be
 * compared with (copy duration)/(writer round duration).
 *
 * argv[1] == "writeback" adds FUSE_WRITEBACK_CACHE.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/fuse.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/mount.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define PAGE_SZ 4096u
#define WORDS_PER_PAGE (PAGE_SZ / 8u) /* 512 */
#define NPAGES 32u
#define FILE_SIZE (NPAGES * PAGE_SZ)
#define NWORDS (NPAGES * WORDS_PER_PAGE) /* 16384 */
#define MAX_IO (1u << 20)
#define MNT "/mnt/v1unsub"
#define NODE_ROOT 1
#define NODE_FILE 2

#define MODE_NONE 0
#define MODE_PAGE0 1
#define MODE_FILE 2
#define MODE_SLOW 3

static int fuse_fd = -1;
static int writeback_mode = 0;
static unsigned char file_bytes[FILE_SIZE];
static volatile unsigned char *g_map;
static unsigned long read_requests, write_requests, write_cache_requests, fsync_requests;
static unsigned char retrieved[MAX_IO];
static uint32_t retrieved_size;
static uint64_t retrieved_cookie;
static unsigned long long drain_word[NWORDS];
static unsigned char drain_seen[NWORDS];
static unsigned long drain_words, drain_writes, drain_cache_writes;
static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static int negotiated_minor = -1, negotiated_flags = -1;
static unsigned negotiated_max_write;
static volatile int writer_stop, writer_mode;
static volatile unsigned long writer_round;
static pthread_t writer_thread;
static int mounted;
static const char *probe_argv1 = "(none)";
static unsigned long long notify_read_ns_max, notify_read_ns_sum, notify_read_count;

static void msleep(int ms) {
    struct timespec ts = {ms / 1000, (long)(ms % 1000) * 1000000L};
    nanosleep(&ts, NULL);
}
static unsigned long long now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (unsigned long long)ts.tv_sec * 1000000000ULL + (unsigned long long)ts.tv_nsec;
}
static void teardown_and_exit(int code) {
    if (mounted) umount2(MNT, MNT_DETACH);
    _exit(code);
}
static void die(const char *what, int code) {
    fprintf(stderr, "probe: FATAL %s (errno=%d %s)\n", what, errno, strerror(errno));
    teardown_and_exit(code);
}
static void on_alarm(int sig) {
    (void)sig;
    write(2, "probe: WATCHDOG timeout\n", 24);
    teardown_and_exit(9);
}
#define CHK(cond) do { if (!(cond)) die("check failed: " #cond, 7); } while (0)

static int reply(uint64_t unique, int error, const void *payload, size_t len) {
    static unsigned char buf[sizeof(struct fuse_out_header) + MAX_IO];
    struct fuse_out_header oh = {.len = (uint32_t)(sizeof(oh) + len),
                                 .error = (int32_t)error,
                                 .unique = unique};
    if (sizeof(oh) + len > sizeof(buf)) return -1;
    pthread_mutex_lock(&g_lock);
    memcpy(buf, &oh, sizeof(oh));
    if (len) memcpy(buf + sizeof(oh), payload, len);
    ssize_t n = write(fuse_fd, buf, sizeof(oh) + len);
    pthread_mutex_unlock(&g_lock);
    return n == (ssize_t)(sizeof(oh) + len) ? 0 : -1;
}

static void fill_attr(struct fuse_attr *a, uint64_t ino, uint32_t mode, uint64_t size) {
    memset(a, 0, sizeof(*a));
    a->ino = ino; a->size = size; a->blocks = (size + 511) / 512;
    a->mode = mode; a->nlink = (mode & S_IFDIR) ? 2 : 1;
    a->blksize = PAGE_SZ;
}

static void *server(void *unused) {
    (void)unused;
    static unsigned char in[sizeof(struct fuse_in_header) + sizeof(struct fuse_write_in) + MAX_IO]
        __attribute__((aligned(8)));
    for (;;) {
        struct pollfd p = {.fd = fuse_fd, .events = POLLIN};
        if (poll(&p, 1, 5) <= 0 || !(p.revents & POLLIN)) continue;
        unsigned long long t0 = now_ns();
        ssize_t n = read(fuse_fd, in, sizeof(in));
        unsigned long long dur = now_ns() - t0;
        if (n <= 0) {
            if (errno == EINTR || errno == EAGAIN) continue;
            fprintf(stderr, "probe: server read %zd errno=%d\n", n, errno);
            return NULL;
        }
        struct fuse_in_header ih;
        if ((size_t)n < sizeof(ih)) continue;
        memcpy(&ih, in, sizeof(ih));
        const unsigned char *arg = in + sizeof(ih);
        size_t arglen = (size_t)n - sizeof(ih);
        if (ih.opcode == FUSE_NOTIFY_REPLY) {
            const struct fuse_notify_retrieve_in *ri = (const void *)arg;
            size_t hdr = sizeof(*ri);
            if (arglen < hdr || arglen != hdr + ri->size || ri->size > sizeof(retrieved))
                die("bad notify reply", 4);
            pthread_mutex_lock(&g_lock);
            memcpy(retrieved, arg + hdr, ri->size);
            retrieved_size = ri->size;
            retrieved_cookie = ih.unique;
            notify_read_ns_sum += dur;
            notify_read_count++;
            if (dur > notify_read_ns_max) notify_read_ns_max = dur;
            pthread_mutex_unlock(&g_lock);
            continue;
        }
        switch (ih.opcode) {
        case FUSE_INIT: {
            const struct fuse_init_in *ii = (const void *)arg;
            struct fuse_init_out io;
            memset(&io, 0, sizeof(io));
            io.major = FUSE_KERNEL_VERSION;
            io.minor = ii->minor < 31 ? ii->minor : 31;
            io.flags = FUSE_BIG_WRITES | FUSE_ASYNC_READ;
            if (writeback_mode) io.flags |= FUSE_WRITEBACK_CACHE;
            io.max_readahead = 0; io.max_write = MAX_IO;
            io.max_background = 16; io.congestion_threshold = 12;
            negotiated_flags = (int)io.flags;
            negotiated_minor = io.minor;
            negotiated_max_write = io.max_write;
            reply(ih.unique, 0, &io, sizeof(io));
            break;
        }
        case FUSE_LOOKUP: {
            const char *name = (const void *)arg;
            if (strcmp(name, "data") != 0) { reply(ih.unique, -ENOENT, NULL, 0); break; }
            struct fuse_entry_out eo;
            memset(&eo, 0, sizeof(eo));
            eo.nodeid = NODE_FILE; eo.entry_valid = 1; eo.attr_valid = 1;
            fill_attr(&eo.attr, NODE_FILE, S_IFREG | 0644, FILE_SIZE);
            reply(ih.unique, 0, &eo, sizeof(eo));
            break;
        }
        case FUSE_GETATTR: {
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            fill_attr(&ao.attr, ih.nodeid,
                      ih.nodeid == NODE_ROOT ? (S_IFDIR | 0755) : (S_IFREG | 0644),
                      ih.nodeid == NODE_ROOT ? 0 : FILE_SIZE);
            reply(ih.unique, 0, &ao, sizeof(ao));
            break;
        }
        case FUSE_OPEN:
        case FUSE_OPENDIR: {
            struct fuse_open_out oo;
            memset(&oo, 0, sizeof(oo));
            oo.fh = 1;
            reply(ih.unique, 0, &oo, sizeof(oo));
            break;
        }
        case FUSE_READ: {
            const struct fuse_read_in *ri = (const void *)arg;
            uint64_t off = ri->offset;
            uint32_t want = ri->size;
            read_requests++;
            if (off >= FILE_SIZE) { reply(ih.unique, 0, NULL, 0); break; }
            if (want > FILE_SIZE - off) want = (uint32_t)(FILE_SIZE - off);
            reply(ih.unique, 0, file_bytes + off, want);
            break;
        }
        case FUSE_WRITE: {
            const struct fuse_write_in *wi = (const void *)arg;
            const unsigned char *data = arg + sizeof(*wi);
            write_requests++;
            if (wi->write_flags & FUSE_WRITE_CACHE) { write_cache_requests++; drain_cache_writes++; }
            drain_writes++;
            if (wi->offset < FILE_SIZE) {
                uint32_t len = wi->size;
                if (len > FILE_SIZE - wi->offset) len = (uint32_t)(FILE_SIZE - wi->offset);
                memcpy(file_bytes + wi->offset, data, len);
                for (unsigned p = 0; p < NPAGES; p++) {
                    uint64_t ps = (uint64_t)p * PAGE_SZ;
                    if (wi->offset > ps || (uint64_t)wi->offset + wi->size < ps + PAGE_SZ) continue;
                    const unsigned char *pp = data + (ps - wi->offset);
                    for (unsigned w = 0; w < WORDS_PER_PAGE; w++) {
                        unsigned long long val;
                        memcpy(&val, pp + (size_t)w * 8, 8);
                        unsigned idx = p * WORDS_PER_PAGE + w;
                        if (val == 0) continue; /* rounds start at 1 */
                        if (!drain_seen[idx]) drain_words++;
                        drain_seen[idx] = 1;
                        drain_word[idx] = val;
                    }
                }
            }
            struct fuse_write_out wo;
            memset(&wo, 0, sizeof(wo));
            wo.size = wi->size;
            reply(ih.unique, 0, &wo, sizeof(wo));
            break;
        }
        case FUSE_FSYNC:
        case FUSE_FSYNCDIR:
            fsync_requests++;
            reply(ih.unique, 0, NULL, 0);
            break;
        case FUSE_SETATTR: {
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            fill_attr(&ao.attr, ih.nodeid, S_IFREG | 0644, FILE_SIZE);
            reply(ih.unique, 0, &ao, sizeof(ao));
            break;
        }
        case FUSE_STATFS: {
            struct fuse_statfs_out so;
            memset(&so, 0, sizeof(so));
            so.st.bsize = PAGE_SZ; so.st.blocks = 4096; so.st.bfree = 2048;
            so.st.bavail = 2048; so.st.files = 16; so.st.ffree = 8; so.st.namelen = 255;
            reply(ih.unique, 0, &so, sizeof(so));
            break;
        }
        case FUSE_FORGET:
        case FUSE_INTERRUPT:
            break;
        case FUSE_DESTROY:
            reply(ih.unique, 0, NULL, 0);
            return NULL;
        default:
            reply(ih.unique, -ENOSYS, NULL, 0);
            break;
        }
    }
    return NULL;
}

/* ------------------------------- writer ---------------------------------- */
static void *writer_main(void *unused) {
    (void)unused;
    unsigned long r = 0;
    while (!writer_stop) {
        r++;
        int mode = writer_mode;
        if (mode == MODE_PAGE0) {
            for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
            }
        } else if (mode == MODE_FILE) {
            for (unsigned i = 0; i < NWORDS; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
            }
        } else if (mode == MODE_SLOW) {
            for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
                usleep(200);
            }
        } else {
            msleep(1);
        }
        writer_round = r;
    }
    return NULL;
}
static void writer_start(int mode) {
    writer_stop = 0; writer_round = 0; writer_mode = mode;
    CHK(pthread_create(&writer_thread, NULL, writer_main, NULL) == 0);
}
static void writer_stop_and_join(void) {
    writer_stop = 1; writer_mode = MODE_NONE;
    CHK(pthread_join(writer_thread, NULL) == 0);
}

/* ------------------------------ analysis --------------------------------- */
struct report {
    unsigned long n;
    unsigned long breaks;            /* adjacent pairs with v[i] < v[i+1] */
    unsigned long breaks_in_page;    /* ... where i % 512 != 511 */
    unsigned long breaks_at_page_end;/* ... where i % 512 == 511 (between pages) */
    unsigned long long max_delta;    /* max (v[newer] - v[older]) over breaks */
    unsigned long first_break_idx;
    unsigned long long first_break_from, first_break_to;
    unsigned long long violations;   /* pairs i<j with v[i] < v[j] (merge sort) */
    unsigned long long min_val, max_val;
    unsigned distinct_values;        /* how many different round values appear */
};

static unsigned long long inv_count;
static unsigned long long *inv_buf;

static void merge_count(unsigned long long *a, unsigned long long *tmp, unsigned lo, unsigned hi) {
    if (hi - lo < 2) return;
    unsigned mid = lo + (hi - lo) / 2;
    merge_count(a, tmp, lo, mid);
    merge_count(a, tmp, mid, hi);
    unsigned i = lo, j = mid, k = lo;
    while (i < mid && j < hi) {
        if (a[i] <= a[j]) tmp[k++] = a[i++];
        else { tmp[k++] = a[j++]; inv_count += (mid - i); }
    }
    while (i < mid) tmp[k++] = a[i++];
    while (j < hi) tmp[k++] = a[j++];
    for (unsigned x = lo; x < hi; x++) a[x] = tmp[x];
}

static int cmp_ull(const void *a, const void *b) {
    unsigned long long x = *(const unsigned long long *)a, y = *(const unsigned long long *)b;
    return x < y ? -1 : (x > y ? 1 : 0);
}

static struct report analyze(const unsigned long long *v, const unsigned char *present,
                             unsigned nwords) {
    struct report r;
    memset(&r, 0, sizeof(r));
    r.min_val = ~0ULL;
    r.first_break_idx = ~0UL;
    static unsigned long long vals[NWORDS];
    unsigned nv = 0;
    unsigned long last = ~0UL;
    for (unsigned i = 0; i < nwords; i++) {
        if (!present[i]) continue;
        r.n++;
        vals[nv++] = v[i];
        if (v[i] < r.min_val) r.min_val = v[i];
        if (v[i] > r.max_val) r.max_val = v[i];
        if (last != ~0UL && v[last] > v[i]) {
            r.breaks++;
            if (i % WORDS_PER_PAGE == WORDS_PER_PAGE - 1) r.breaks_at_page_end++;
            else r.breaks_in_page++;
            unsigned long long d = v[last] - v[i]; /* older value is the smaller one */
            if (d > r.max_delta) r.max_delta = d;
            if (r.first_break_idx == ~0UL) {
                r.first_break_idx = i;
                r.first_break_from = v[last];
                r.first_break_to = v[i];
            }
        }
        last = i;
    }
    if (r.min_val == ~0ULL) r.min_val = 0;
    if (nv) {
        qsort(vals, nv, sizeof(unsigned long long), cmp_ull);
        r.distinct_values = 1;
        for (unsigned i = 1; i < nv; i++) if (vals[i] != vals[i - 1]) r.distinct_values++;
    }
    static unsigned long long seq[NWORDS];
    unsigned k = 0;
    for (unsigned i = 0; i < nwords; i++) if (present[i]) seq[k++] = v[i];
    inv_count = 0;
    if (k > 1) merge_count(seq, inv_buf, 0, k);
    r.violations = inv_count;
    return r;
}

static void words_from_retrieved(unsigned long long *v, unsigned char *present, unsigned nwords,
                                 unsigned offset_words) {
    unsigned n = retrieved_size / 8;
    for (unsigned i = 0; i < nwords; i++) {
        v[i] = 0; present[i] = 0;
        unsigned src = offset_words + i;
        if (src * 8 + 8 <= retrieved_size && src < n) {
            memcpy(&v[i], retrieved + (size_t)src * 8, 8);
            present[i] = 1;
        }
    }
    (void)n;
}

static uint64_t next_cookie = 100;
static void retrieve(uint64_t nodeid, uint64_t cookie, uint64_t offset, uint32_t size) {
    struct fuse_notify_retrieve_out r;
    memset(&r, 0, sizeof(r));
    r.notify_unique = cookie; r.nodeid = nodeid; r.offset = offset; r.size = size;
    CHK(reply(0, FUSE_NOTIFY_RETRIEVE, &r, sizeof(r)) == 0);
}
static int wait_retrieve(uint64_t cookie) {
    for (int i = 0; i < 5000; i++) {
        pthread_mutex_lock(&g_lock);
        int done = (retrieved_cookie == cookie);
        pthread_mutex_unlock(&g_lock);
        if (done) return 0;
        msleep(1);
    }
    return -1;
}

/* One retrieve trial: returns the report over the first `an_words` words of the
 * reply (the words of the swept page for MODE_PAGE0). */
static struct report one_trial(uint64_t offset, uint32_t size, unsigned an_words,
                               unsigned long *rounds_elapsed) {
    static unsigned long long v[NWORDS];
    static unsigned char present[NWORDS];
    uint64_t c = next_cookie++;
    unsigned long rr0 = writer_round;
    retrieve(NODE_FILE, c, offset, size);
    CHK(wait_retrieve(c) == 0);
    unsigned long rr1 = writer_round;
    if (rounds_elapsed) *rounds_elapsed = rr1 - rr0;
    words_from_retrieved(v, present, an_words, 0);
    return analyze(v, present, an_words);
}

static int main_experiments(void) {
    fprintf(stderr, "probe: argv1=%s writeback_mode=%d\n", probe_argv1, writeback_mode);
    CHK(mkdir(MNT, 0755) == 0 || errno == EEXIST);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    if (fuse_fd < 0) { perror("open /dev/fuse"); return 2; }
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", fuse_fd, S_IFDIR,
             getuid(), getgid());
    if (mount("v1unsub", MNT, "fuse", MS_NOSUID | MS_NODEV, opts) != 0) { perror("mount"); return 2; }
    mounted = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t sthread;
    CHK(pthread_create(&sthread, NULL, server, NULL) == 0);

    char path[256];
    snprintf(path, sizeof(path), "%s/data", MNT);
    int fd = open(path, O_RDWR);
    if (fd < 0) { perror("open data"); return 3; }
    g_map = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (g_map == MAP_FAILED) { perror("mmap data"); return 3; }
    printf("probe: page_size=%ld file_size=%u pages=%u words=%u mmap_shared_write=ok\n",
           sysconf(_SC_PAGESIZE), FILE_SIZE, NPAGES, NWORDS);
    printf("probe: negotiated minor=%d flags=0x%x max_write=%u writeback_cache=%u\n",
           negotiated_minor, negotiated_flags, negotiated_max_write,
           (negotiated_flags & FUSE_WRITEBACK_CACHE) ? 1u : 0u);

    /* prefault + dirty every word through the mapping, then quiesce */
    writer_start(MODE_FILE);
    msleep(200);
    writer_stop_and_join();
    printf("probe: prefault done rounds=%lu daemon_reads=%lu daemon_writes=%lu\n", writer_round,
           read_requests, write_requests);

    /* writer round rate per mode */
    for (int mode = MODE_PAGE0; mode <= MODE_FILE; mode++) {
        writer_start(mode);
        unsigned long r0 = writer_round;
        msleep(100);
        writer_stop_and_join();
        unsigned long rounds = writer_round - r0;
        unsigned words = (mode == MODE_PAGE0) ? WORDS_PER_PAGE : NWORDS;
        printf("probe: writer_mode=%s round_words=%u rounds_per_100ms=%lu ns_per_round=%lu "
               "ns_per_word=%lu\n",
               mode == MODE_PAGE0 ? "page0" : "file", words, rounds,
               rounds ? 100000000UL / rounds : 0,
               rounds ? 100000000UL / rounds / words : 0);
    }

    /* ---- T-SP-A: one page, page-confined writer (decisive geometry) ------- */
    {
        unsigned trials = 0, torn = 0;
        unsigned long long max_delta = 0, max_viol = 0;
        unsigned long inpage = 0, pageend = 0;
        unsigned long long max_rounds = 0;
        writer_start(MODE_PAGE0);
        msleep(50);
        for (unsigned t = 0; t < 200; t++) {
            unsigned long rounds = 0;
            struct report rep = one_trial(0, PAGE_SZ, WORDS_PER_PAGE, &rounds);
            trials++;
            if (rounds > max_rounds) max_rounds = rounds;
            if (rep.breaks || rep.violations) {
                torn++;
                inpage += rep.breaks_in_page;
                pageend += rep.breaks_at_page_end;
                if (rep.max_delta > max_delta) max_delta = rep.max_delta;
                if (rep.violations > max_viol) max_viol = rep.violations;
                if (torn <= 12)
                    printf("T-SP-A trial=%u writer_rounds_during_reply=%lu breaks=%lu in_page=%lu "
                           "first_break_word=%lu delta=%llu violating_pairs=%llu distinct_rounds=%u "
                           "sample_range=%llu..%llu\n",
                           t, rounds, rep.breaks, rep.breaks_in_page, rep.first_break_idx,
                           rep.max_delta, rep.violations, rep.distinct_values, rep.min_val,
                           rep.max_val);
            }
        }
        writer_stop_and_join();
        printf("RESULT T-SP-A mode=page0 retrieve=4096B trials=%u single_page_replies_torn=%u "
               "tear_rate_pct=%lu max_delta_rounds=%llu max_violating_pairs=%llu "
               "breaks_in_page=%lu breaks_at_page_end=%lu max_writer_rounds_during_reply=%llu\n",
               trials, torn, trials ? torn * 100UL / trials : 0, max_delta, max_viol, inpage,
               pageend, max_rounds);
    }

    /* ---- T-SP-B: full 128 KiB reply, page-confined writer ----------------- */
    {
        unsigned trials = 0, torn = 0;
        unsigned long long max_delta = 0, max_viol = 0;
        unsigned long inpage = 0, pageend = 0;
        writer_start(MODE_PAGE0);
        msleep(50);
        for (unsigned t = 0; t < 20; t++) {
            unsigned long rounds = 0;
            struct report rep = one_trial(0, NPAGES * PAGE_SZ, WORDS_PER_PAGE, &rounds);
            trials++;
            if (rep.breaks || rep.violations) {
                torn++;
                inpage += rep.breaks_in_page;
                pageend += rep.breaks_at_page_end;
                if (rep.max_delta > max_delta) max_delta = rep.max_delta;
                if (rep.violations > max_viol) max_viol = rep.violations;
            }
            printf("T-SP-B trial=%u reply_bytes=%u writer_rounds_during_reply=%lu breaks=%lu "
                   "in_page=%lu first_break_word=%lu delta=%llu violating_pairs=%llu "
                   "distinct_rounds=%u sample_range=%llu..%llu\n",
                   t, retrieved_size, rounds, rep.breaks, rep.breaks_in_page, rep.first_break_idx,
                   rep.max_delta, rep.violations, rep.distinct_values, rep.min_val, rep.max_val);
        }
        writer_stop_and_join();
        printf("RESULT T-SP-B mode=page0 retrieve=131072B trials=%u replies_with_a_sub-page_tear=%u "
               "max_delta_rounds=%llu max_violating_pairs=%llu breaks_in_page=%lu "
               "breaks_at_page_end=%lu (page0 analysed: 512 words)\n",
               trials, torn, max_delta, max_viol, inpage, pageend);
    }

    /* ---- T-SP-C: quiesced controls --------------------------------------- */
    {
        struct report one = one_trial(0, PAGE_SZ, WORDS_PER_PAGE, NULL);
        struct report full = one_trial(0, NPAGES * PAGE_SZ, WORDS_PER_PAGE, NULL);
        printf("RESULT T-SP-C quiesced_page_breaks=%lu quiesced_page_violations=%llu "
               "quiesced_page_distinct_rounds=%u quiesced_full_page0_breaks=%lu "
               "quiesced_full_page0_violations=%llu quiesced_full_page0_range=%llu..%llu\n",
               one.breaks, one.violations, one.distinct_values, full.breaks, full.violations,
               full.min_val, full.max_val);
    }

    /* ---- T-SP-D: slow-writer control (no straddle possible) -------------- */
    {
        unsigned trials = 0, torn = 0;
        writer_start(MODE_SLOW);
        msleep(300);
        for (unsigned t = 0; t < 100; t++) {
            struct report rep = one_trial(0, PAGE_SZ, WORDS_PER_PAGE, NULL);
            trials++;
            if (rep.breaks || rep.violations) torn++;
        }
        writer_stop_and_join();
        printf("RESULT T-SP-D slow_writer_control trials=%u torn=%u (200us between words: a 4-KiB "
               "page copy cannot straddle a store)\n",
               trials, torn);
    }

    /* ---- T-SP-E: whole-file sweep geometry (version 1, delta fixed) ------- */
    {
        writer_start(MODE_FILE);
        msleep(50);
        unsigned trials = 0, torn_page = 0, torn_full = 0;
        unsigned long long max_delta_page = 0, max_delta_full = 0;
        for (unsigned t = 0; t < 200; t++) {
            struct report rep = one_trial(0, PAGE_SZ, WORDS_PER_PAGE, NULL);
            trials++;
            if (rep.breaks || rep.violations) {
                torn_page++;
                if (rep.max_delta > max_delta_page) max_delta_page = rep.max_delta;
                if (torn_page <= 8)
                    printf("T-SP-E single_page trial=%u breaks=%lu first_break_word=%lu delta=%llu "
                           "violating_pairs=%llu distinct_rounds=%u range=%llu..%llu\n",
                           t, rep.breaks, rep.first_break_idx, rep.max_delta, rep.violations,
                           rep.distinct_values, rep.min_val, rep.max_val);
            }
        }
        for (unsigned t = 0; t < 20; t++) {
            struct report rep = one_trial(0, NPAGES * PAGE_SZ, NWORDS, NULL);
            if (rep.breaks || rep.violations) {
                torn_full++;
                if (rep.max_delta > max_delta_full) max_delta_full = rep.max_delta;
            }
        }
        writer_stop_and_join();
        printf("RESULT T-SP-E mode=file single_page_trials=%u single_page_torn=%u "
               "single_page_max_delta=%llu full_reply_trials=20 full_reply_torn=%u "
               "full_reply_max_delta=%llu (version-1 geometry: writer and copier run at the same "
               "words/ns, so the full reply stays in phase)\n",
               trials, torn_page, max_delta_page, torn_full, max_delta_full);
    }

    /* ---- T-SP-F: writeback drain payload, word granularity --------------- */
    {
        int self = open(path, O_RDWR);
        if (self < 0) { perror("daemon self-open"); return 5; }
        writer_start(MODE_PAGE0);
        msleep(50);
        unsigned drain_torn = 0, drain_trials = 0;
        unsigned long long drain_max_delta = 0;
        for (unsigned t = 0; t < 10; t++) {
            memset(drain_seen, 0, sizeof(drain_seen));
            memset(drain_word, 0, sizeof(drain_word));
            drain_words = drain_writes = drain_cache_writes = 0;
            unsigned long wr0 = write_requests;
            unsigned long rr0 = writer_round;
            int rc = fsync(self);
            unsigned long rr1 = writer_round;
            struct report rep = analyze(drain_word, drain_seen, WORDS_PER_PAGE);
            drain_trials++;
            if (rep.breaks || rep.violations) {
                drain_torn++;
                if (rep.max_delta > drain_max_delta) drain_max_delta = rep.max_delta;
            }
            printf("T-SP-F trial=%u fsync_rc=%d writer_rounds_during_fsync=%lu daemon_writes=%lu "
                   "cache_writes=%lu page0_words_delivered=%lu breaks=%lu in_page=%lu delta=%llu "
                   "violating_pairs=%llu distinct_rounds=%u range=%llu..%llu\n",
                   t, rc, rr1 - rr0, write_requests - wr0, drain_cache_writes, rep.n, rep.breaks,
                   rep.breaks_in_page, rep.max_delta, rep.violations, rep.distinct_values,
                   rep.min_val, rep.max_val);
        }
        printf("RESULT T-SP-F drain_trials=%u drain_page0_replies_torn=%u drain_max_delta_rounds=%llu "
               "(page 0 word-granular analysis of the FUSE_WRITE payload)\n",
               drain_trials, drain_torn, drain_max_delta);
        writer_stop_and_join();
        close(self);
    }

    printf("probe: retrieve_reply_read_duration_ns count=%llu max=%llu mean=%llu\n",
           notify_read_count, notify_read_ns_max,
           notify_read_count ? notify_read_ns_sum / notify_read_count : 0);
    printf("probe: done. daemon_totals reads=%lu writes=%lu write_cache=%lu fsync=%lu\n",
           read_requests, write_requests, write_cache_requests, fsync_requests);
    munmap((void *)g_map, FILE_SIZE);
    close(fd);
    msleep(100);
    alarm(0);
    mounted = 0;
    umount2(MNT, MNT_DETACH);
    return 0;
}

int main(int argc, char **argv) {
    struct rlimit rl = {0, 0};
    setrlimit(RLIMIT_CORE, &rl);
    signal(SIGALRM, on_alarm);
    alarm(300);
    if (argc > 1) probe_argv1 = argv[1];
    writeback_mode = (argc > 1 && strcmp(argv[1], "writeback") == 0);
    inv_buf = calloc(NWORDS, sizeof(unsigned long long));
    if (!inv_buf) return 2;
    return main_experiments();
}
