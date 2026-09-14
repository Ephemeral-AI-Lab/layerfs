/* v1-probe-unmeasured / SUB-PAGE TEARING probe, VERSION 1 (whole-file sweep).
 *
 * PRESERVED FIRST VERSION. Raw output: run-subpage.log (+ run-subpage.json).
 * This version's writer sweeps the whole 16384-word file per round; its
 * per-word store rate (~0.82 ns/word) is the same order as the kernel's page
 * copy rate, so copier and writer ran nearly in lockstep and the 128-KiB reply
 * came back uniform (T-SP1: 0/20 torn, sample_range shows a single round). It is
 * kept as the raw evidence for that measurement; superseded for the sub-page
 * question by subpage.c (page-confined writer). One real bug in this version is
 * annotated in analyze() below.
 *
 * Original header follows.
 *
 * == SUB-PAGE TEARING probe.
 *
 * Question left explicitly untested by v1-probe-generic: can a single
 * FUSE_NOTIFY_RETRIEVE reply be a *mixed-instant* image at sub-page (8-byte)
 * granularity? The earlier probe compared only the first 8 bytes of each page,
 * so it could only see tearing at 4-KiB granularity.
 *
 * Oracle (finer than the page oracle, and a strict superset of it):
 *   The writer sweeps every aligned 8-byte word of the file in ascending global
 *   order -- page 0 word 0 .. page 0 word 511, page 1 word 0, ... -- storing the
 *   round number r at each word and issuing a full barrier (__sync_synchronize,
 *   DMB on aarch64) between stores. One round completes before the next begins.
 *   Therefore at *every* instant, for any two global word indices i < j,
 *       value(word i) >= value(word j)
 *   (word i was written earlier in the current round or in a later round than
 *   word j). Any collected sample with v[i] < v[j] for some i < j is not the
 *   file's content at any instant.
 *   A break with i % 512 != 511 is a break *inside a single 4-KiB page*: the
 *   copy of that one page straddled a writer round boundary.
 *
 * The same oracle applies to a writeback drain payload (WRITE with
 * FUSE_WRITE_CACHE), so T-SP5 repeats the analysis on the drain path. Nothing
 * here depends on the copy being issued in ascending order: an ascending copy
 * still yields violations (measured: the writer's round advances between the
 * copies of two adjacent words).
 *
 * No libfuse; one cached FUSE session over /dev/fuse mounted with mount(2).
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
static unsigned long drain_pages, drain_writes, drain_cache_writes;
static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static int negotiated_minor = -1, negotiated_flags = -1;
static unsigned negotiated_max_write;
static volatile int writer_stop, writer_mode, writer_slow;
static volatile unsigned long writer_round;
static pthread_t writer_thread;
static int mounted;
static const char *probe_argv1 = "(none)";

static void msleep(int ms) {
    struct timespec ts = {ms / 1000, (long)(ms % 1000) * 1000000L};
    nanosleep(&ts, NULL);
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
        ssize_t n = read(fuse_fd, in, sizeof(in));
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
            fill_attr(&ao.attr, ih.nodeid, ih.nodeid == NODE_ROOT ? (S_IFDIR | 0755)
                                                                  : (S_IFREG | 0644),
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
                /* word-level capture of the delivered payload */
                for (unsigned p = 0; p < NPAGES; p++) {
                    uint64_t ps = (uint64_t)p * PAGE_SZ;
                    if (wi->offset > ps || (uint64_t)wi->offset + wi->size < ps + PAGE_SZ) continue;
                    const unsigned char *pp = data + (ps - wi->offset);
                    for (unsigned w = 0; w < WORDS_PER_PAGE; w++) {
                        unsigned long long val;
                        memcpy(&val, pp + (size_t)w * 8, 8);
                        unsigned idx = p * WORDS_PER_PAGE + w;
                        if (val == 0) continue; /* writer's round starts at 1 */
                        if (!drain_seen[idx]) drain_pages++;
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
/* One round writes every aligned word of the file once, in ascending global
 * word order, with a full barrier after each store. */
static void *writer_main(void *unused) {
    (void)unused;
    unsigned long r = 0;
    while (!writer_stop) {
        r++;
        if (writer_slow) {
            /* slow control: 200 us between words => the copy of one 4-KiB page
             * (a few hundred ns) cannot straddle a store boundary at all. */
            for (unsigned i = 0; i < NWORDS; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
                usleep(200);
            }
        } else {
            for (unsigned i = 0; i < NWORDS; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
            }
        }
        writer_round = r;
    }
    return NULL;
}
static void writer_start(int slow) {
    writer_stop = 0; writer_round = 0; writer_slow = slow;
    CHK(pthread_create(&writer_thread, NULL, writer_main, NULL) == 0);
}
static void writer_stop_and_join(void) {
    writer_stop = 1;
    CHK(pthread_join(writer_thread, NULL) == 0);
}

/* ------------------------------ analysis --------------------------------- */
struct report {
    unsigned long n;          /* words examined */
    unsigned long breaks;     /* adjacent pairs with v[i] < v[i+1] */
    unsigned long breaks_in_page;   /* ... where i % 512 != 511 */
    unsigned long breaks_at_page_end; /* ... where i % 512 == 511 (page boundary) */
    unsigned long long max_delta;
    unsigned long first_break_idx;
    unsigned long long first_break_from, first_break_to;
    unsigned long long inversions; /* pairs i<j with v[i] < v[j] (merge sort) */
    unsigned long long min_val, max_val;
};

static unsigned long long inv_count;
static unsigned long long *inv_buf;

static void merge_count(unsigned long long *a, unsigned long long *tmp, unsigned lo,
                        unsigned hi) {
    if (hi - lo < 2) return;
    unsigned mid = lo + (hi - lo) / 2;
    merge_count(a, tmp, lo, mid);
    merge_count(a, tmp, mid, hi);
    unsigned i = lo, j = mid, k = lo;
    while (i < mid && j < hi) {
        if (a[i] <= a[j]) { tmp[k++] = a[i++]; }
        else { tmp[k++] = a[j++]; inv_count += (mid - i); }
    }
    while (i < mid) tmp[k++] = a[i++];
    while (j < hi) tmp[k++] = a[j++];
    for (unsigned x = lo; x < hi; x++) a[x] = tmp[x];
}

static struct report analyze(const unsigned long long *v, const unsigned char *present,
                             unsigned nwords) {
    struct report r;
    memset(&r, 0, sizeof(r));
    r.min_val = ~0ULL;
    r.first_break_idx = ~0UL;
    unsigned long last = ~0UL;
    for (unsigned i = 0; i < nwords; i++) {
        if (!present[i]) continue;
        r.n++;
        if (v[i] < r.min_val) r.min_val = v[i];
        if (v[i] > r.max_val) r.max_val = v[i];
        if (last != ~0UL && v[last] > v[i]) {
            r.breaks++;
            if (i % WORDS_PER_PAGE == WORDS_PER_PAGE - 1) r.breaks_at_page_end++;
            else r.breaks_in_page++;
            unsigned long long d = v[last] - v[i]; /* BUG (v1 only; fixed in subpage.c):
                                                    * v1 computed v[i]-v[last] under the
                                                    * branch v[last]>v[i], so printed
                                                    * UINT64_MAX as max_delta */
            if (d > r.max_delta) r.max_delta = d;
            if (r.first_break_idx == ~0UL) {
                /* record the first break: `last` holds the newer value */
                r.first_break_idx = i;
                r.first_break_from = v[last];
                r.first_break_to = v[i];
            }
        }
        last = i;
    }
    if (r.min_val == ~0ULL) r.min_val = 0;
    /* inversion count over the present words, in order */
    static unsigned long long seq[NWORDS];
    unsigned k = 0;
    for (unsigned i = 0; i < nwords; i++) if (present[i]) seq[k++] = v[i];
    inv_count = 0;
    if (k > 1) { merge_count(seq, inv_buf, 0, k); }
    r.inversions = inv_count;
    return r;
}

static void npages_from_retrieved(unsigned long long *v, unsigned char *present, unsigned *nwords) {
    unsigned n = retrieved_size / 8;
    if (n > NWORDS) n = NWORDS;
    for (unsigned i = 0; i < NWORDS; i++) {
        present[i] = 0; v[i] = 0;
        if (i < n) { memcpy(&v[i], retrieved + (size_t)i * 8, 8); present[i] = 1; }
    }
    *nwords = n;
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
    printf("probe: page_size=%ld file_size=%u pages=%u words=%u mmap=ok\n", sysconf(_SC_PAGESIZE),
           FILE_SIZE, NPAGES, NWORDS);
    printf("probe: negotiated minor=%d flags=0x%x max_write=%u writeback_cache=%u\n",
           negotiated_minor, negotiated_flags, negotiated_max_write,
           (negotiated_flags & FUSE_WRITEBACK_CACHE) ? 1u : 0u);

    /* prefault + dirty every word through the mapping, then quiesce */
    writer_start(0);
    msleep(200);
    writer_stop_and_join();
    printf("probe: prefault done rounds=%lu daemon_reads=%lu daemon_writes=%lu\n", writer_round,
           read_requests, write_requests);

    /* writer rate: rounds (one pass over all %u words) per 100 ms */
    {
        writer_start(0);
        unsigned long r0 = writer_round;
        msleep(100);
        writer_stop_and_join();
        unsigned long rounds = writer_round - r0;
        printf("probe: writer_rate=%lu rounds/100ms = %lu ns/round, %lu ns/word "
               "(1 round = %u aligned 8-byte stores, each followed by a full barrier)\n",
               rounds, rounds ? 100000000UL / rounds : 0,
               rounds ? 100000000UL / rounds / NWORDS : 0, NWORDS);
    }

    /* ---- T-SP1: full 128 KiB retrieve, word-granular analysis ------------- */
    {
        unsigned trials = 0, torn = 0;
        unsigned long long max_delta = 0, max_inv = 0;
        unsigned long inpage_breaks = 0, pageend_breaks = 0;
        writer_start(0);
        msleep(50);
        for (unsigned t = 0; t < 20; t++) {
            uint64_t c = next_cookie++;
            unsigned long rr0 = writer_round;
            retrieve(NODE_FILE, c, 0, NPAGES * PAGE_SZ);
            CHK(wait_retrieve(c) == 0);
            unsigned long rr1 = writer_round;
            static unsigned long long v[NWORDS];
            static unsigned char present[NWORDS];
            unsigned nwords = 0;
            npages_from_retrieved(v, present, &nwords);
            struct report rep = analyze(v, present, NWORDS);
            trials++;
            if (rep.breaks || rep.inversions) torn++;
            if (rep.max_delta > max_delta) max_delta = rep.max_delta;
            if (rep.inversions > max_inv) max_inv = rep.inversions;
            inpage_breaks += rep.breaks_in_page;
            pageend_breaks += rep.breaks_at_page_end;
            printf("T-SP1 trial=%u words=%u bytes=%u writer_rounds_during_reply=%lu "
                   "adjacent_order_breaks=%lu (in_page=%lu at_page_end=%lu) "
                   "first_break_word_global=%lu page=%lu word_in_page=%lu delta=%llu "
                   "violating_pairs=%llu sample_range=%llu..%llu\n",
                   t, nwords, retrieved_size, rr1 - rr0, rep.breaks, rep.breaks_in_page,
                   rep.breaks_at_page_end, rep.first_break_idx,
                   rep.first_break_idx / WORDS_PER_PAGE, rep.first_break_idx % WORDS_PER_PAGE,
                   rep.first_break_to - rep.first_break_from, rep.inversions, rep.min_val,
                   rep.max_val);
        }
        writer_stop_and_join();
        printf("RESULT T-SP1 trials=%u replies_torn_at_8byte_granularity=%u max_delta_rounds=%llu "
               "max_violating_pairs=%llu breaks_in_page_total=%lu breaks_at_page_end_total=%lu\n",
               trials, torn, max_delta, max_inv, inpage_breaks, pageend_breaks);
    }

    /* ---- T-SP2: a single 4-KiB page retrieve, word granularity ------------ */
    {
        unsigned trials = 0, torn = 0;
        unsigned long long max_delta = 0;
        unsigned long inpage = 0;
        writer_start(0);
        msleep(50);
        for (unsigned t = 0; t < 200; t++) {
            uint64_t c = next_cookie++;
            unsigned long rr0 = writer_round;
            retrieve(NODE_FILE, c, 0, PAGE_SZ);
            CHK(wait_retrieve(c) == 0);
            unsigned long rr1 = writer_round;
            static unsigned long long v[NWORDS];
            static unsigned char present[NWORDS];
            for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
                memcpy(&v[i], retrieved + (size_t)i * 8, 8);
                present[i] = 1;
            }
            struct report rep = analyze(v, present, WORDS_PER_PAGE);
            trials++;
            if (rep.breaks || rep.inversions) {
                torn++;
                inpage += rep.breaks_in_page;
                if (rep.max_delta > max_delta) max_delta = rep.max_delta;
                printf("T-SP2 trial=%u writer_rounds_during_reply=%lu breaks=%lu in_page=%lu "
                       "first_break_word=%lu delta=%llu violating_pairs=%llu sample_range=%llu..%llu\n",
                       t, rr1 - rr0, rep.breaks, rep.breaks_in_page, rep.first_break_idx,
                       rep.first_break_to - rep.first_break_from, rep.inversions, rep.min_val,
                       rep.max_val);
            }
        }
        writer_stop_and_join();
        printf("RESULT T-SP2 trials=%u single_page_replies_torn_at_8byte_granularity=%u "
               "in_page_breaks_total=%lu max_delta_rounds=%llu\n",
               trials, torn, inpage, max_delta);
    }

    /* ---- T-SP3: quiesced controls ---------------------------------------- */
    {
        uint64_t c = next_cookie++;
        retrieve(NODE_FILE, c, 0, NPAGES * PAGE_SZ);
        CHK(wait_retrieve(c) == 0);
        static unsigned long long v[NWORDS];
        static unsigned char present[NWORDS];
        unsigned nwords = 0;
        npages_from_retrieved(v, present, &nwords);
        struct report full = analyze(v, present, NWORDS);
        printf("T-SP3C quiesced_full words=%u breaks=%lu in_page=%lu violations=%llu "
               "sample_range=%llu..%llu\n",
               nwords, full.breaks, full.breaks_in_page, full.inversions, full.min_val, full.max_val);
        c = next_cookie++;
        retrieve(NODE_FILE, c, 0, PAGE_SZ);
        CHK(wait_retrieve(c) == 0);
        for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
            memcpy(&v[i], retrieved + (size_t)i * 8, 8);
            present[i] = 1;
        }
        struct report one = analyze(v, present, WORDS_PER_PAGE);
        printf("RESULT T-SP3C quiesced_full_breaks=%lu quiesced_full_violating_pairs=%llu "
               "quiesced_page_breaks=%lu quiesced_page_violating_pairs=%llu\n",
               full.breaks, full.inversions, one.breaks, one.inversions);
    }

    /* ---- T-SP4: slow-writer control: a page copy cannot straddle a store -- */
    {
        unsigned trials = 0, torn = 0;
        writer_start(1); /* 200 us between words */
        msleep(300);
        for (unsigned t = 0; t < 100; t++) {
            uint64_t c = next_cookie++;
            retrieve(NODE_FILE, c, 0, PAGE_SZ);
            CHK(wait_retrieve(c) == 0);
            static unsigned long long v[NWORDS];
            static unsigned char present[NWORDS];
            for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
                memcpy(&v[i], retrieved + (size_t)i * 8, 8);
                present[i] = 1;
            }
            struct report rep = analyze(v, present, WORDS_PER_PAGE);
            trials++;
            if (rep.breaks || rep.inversions) torn++;
        }
        unsigned long rounds = writer_round;
        writer_stop_and_join();
        printf("RESULT T-SP4 slow_writer_control trials=%u torn=%u writer_rounds_seen=%lu "
               "(200us/word: the copy of one page cannot straddle a store)\n",
               trials, torn, rounds);
    }

    /* ---- T-SP5: the writeback-drain payload, word granularity ------------- */
    {
        int self = open(path, O_RDWR);
        if (self < 0) { perror("daemon self-open"); return 5; }
        writer_start(0);
        msleep(50);
        memset(drain_seen, 0, sizeof(drain_seen));
        memset(drain_word, 0, sizeof(drain_word));
        drain_pages = drain_writes = drain_cache_writes = 0;
        unsigned long wr0 = write_requests;
        unsigned long rr0 = writer_round;
        int rc = fsync(self);
        unsigned long rr1 = writer_round;
        unsigned long wr1 = write_requests;
        /* run words are 1..r, so 0 means "not delivered"; the writer starts at 1
         * and the file was never zero-only, so treat 0 as absent. */
        struct report rep = analyze(drain_word, drain_seen, NWORDS);
        printf("T-SP5 fsync_rc=%d writer_rounds_during_fsync=%lu daemon_writes=%lu "
               "write_cache_writes=%lu words_seen=%lu/%u breaks=%lu (in_page=%lu at_page_end=%lu) "
               "first_break_word_global=%lu page=%lu word_in_page=%lu delta=%llu "
               "violating_pairs=%llu\n",
               rc, rr1 - rr0, wr1 - wr0, drain_cache_writes, rep.n, NWORDS, rep.breaks,
               rep.breaks_in_page, rep.breaks_at_page_end, rep.first_break_idx,
               rep.first_break_idx / WORDS_PER_PAGE, rep.first_break_idx % WORDS_PER_PAGE,
               rep.first_break_to - rep.first_break_from, rep.inversions);
        printf("RESULT T-SP5 drain_words_delivered=%lu drain_adjacent_breaks=%lu "
               "drain_breaks_in_page=%lu drain_violating_pairs=%llu\n",
               rep.n, rep.breaks, rep.breaks_in_page, rep.inversions);
        writer_stop_and_join();
        close(self);
    }

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
    alarm(150);
    if (argc > 1) probe_argv1 = argv[1];
    writeback_mode = (argc > 1 && strcmp(argv[1], "writeback") == 0);
    inv_buf = calloc(NWORDS, sizeof(unsigned long long));
    if (!inv_buf) return 2;
    return main_experiments();
}
