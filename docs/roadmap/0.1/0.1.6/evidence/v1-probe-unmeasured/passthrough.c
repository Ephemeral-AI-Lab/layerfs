/* v1-probe-unmeasured / FUSE_PASSTHROUGH probe (mechanism 1).
 *
 * Questions:
 *   PAS-1  with FUSE_PASSTHROUGH negotiated and a backing file registered for
 *          the inode, does a writable MAP_SHARED mapping on the passthrough file
 *          work, and where do its dirty bytes live? Can the daemon read them
 *          (a) without any cooperation from the mapping process, (b) without an
 *          implicit fsync, and (c) with zero FUSE requests?
 *   PAS-2  is that read a *single-instant* image (the contract's requirement)?
 *          Word-granular oracle, same as subpage.c: the writer sweeps the 512
 *          aligned 8-byte words of page 0 in ascending order with a full barrier
 *          between stores, so at every instant v[i] >= v[j] for i < j.
 *   PAS-2b the same read after fsync(backing fd) -- does the daemon's own flush
 *          turn the read into a single instant?
 *   PAS-3  does the read stall the mapped writer?
 *   PAS-4  descriptor/storage cost: how many descriptors does the daemon hold per
 *          inode (the contract resolution says "one backing file per inode,
 *          unbounded descriptors")?
 *   PAS-5  does it compose with the existing cached-open surface? Kernel:
 *          fs/fuse/iomode.c:211-216 sends the open to the *cached* path for an
 *          inode that already has a backing, and fs/fuse/file.c:2605-2612 refuses
 *          a cached mmap while an inode has a backing.
 *   PAS-6  open-unlinked: does a passthrough mapping survive unlink?
 *   PAS-7  total FUSE READ/WRITE requests observed for mapped passthrough I/O.
 *
 * Kernel facts this probe exercises (v6.12.76, cited in kernel-citations.txt):
 *   - fs/fuse/passthrough.c:213-269 fuse_backing_open: the daemon registers a
 *     real file fd on its /dev/fuse fd with FUSE_DEV_IOC_BACKING_OPEN
 *     (`!fc->passthrough || !capable(CAP_SYS_ADMIN)` -> -EPERM).
 *   - fs/fuse/iomode.c:168-194: FOPEN_PASSTHROUGH in an OPEN reply binds that
 *     open to backing_id; fs/fuse/file.c:2593-2612: mmap of a passthrough file
 *     is redirected to the backing file (`fuse_passthrough_mmap` ->
 *     `backing_file_mmap`), and a cached mmap is refused (-ENODEV) once the
 *     inode has a backing.
 *   - fs/fuse/passthrough.c:328-330 "Allocate backing file per fuse file".
 *
 * No libfuse; one session over /dev/fuse mounted with mount(2) under
 * CAP_SYS_ADMIN, one process that is both daemon and client (like the earlier
 * v1-probe-generic harness).
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
#include <dirent.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/mount.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define PAGE_SZ 4096u
#define WORDS_PER_PAGE (PAGE_SZ / 8u)
#define NPAGES 32u
#define FILE_SIZE (NPAGES * PAGE_SZ)
#define NWORDS (NPAGES * WORDS_PER_PAGE)
#define MAX_IO (1u << 20)
#define MNT "/mnt/v1unpas"
#define NODE_ROOT 1
#define NFILES 4
#define NODE_BASE 2 /* data, data2, data3, data4 -> nodeid 2..5 */

/* 6.12 uapi additions missing from the 6.1 container headers */
#define UN_FUSE_INIT_EXT     (1u << 30)
#define UN_FUSE_PASSTHROUGH  (1ULL << 37)
#define UN_FOPEN_PASSTHROUGH (1 << 7)

struct un_fuse_init_out {
    uint32_t major, minor, max_readahead, flags;
    uint16_t max_background, congestion_threshold;
    uint32_t max_write, time_gran;
    uint16_t max_pages, map_alignment;
    uint32_t flags2;
    uint32_t max_stack_depth;
    uint32_t unused[6];
};
struct un_fuse_open_out {
    uint64_t fh;
    uint32_t open_flags;
    int32_t backing_id;
};
struct un_fuse_backing_map {
    int32_t fd;
    uint32_t flags;
    uint64_t padding;
};
#define UN_FUSE_DEV_IOC_MAGIC 229
#define UN_FUSE_DEV_IOC_BACKING_OPEN \
    _IOW(UN_FUSE_DEV_IOC_MAGIC, 1, struct un_fuse_backing_map)

static const char *file_names[NFILES] = {"data", "data2", "data3", "data4"};
static int backing_fd[NFILES] = {-1, -1, -1, -1};
static int backing_id[NFILES] = {0, 0, 0, 0};
static volatile int cached_reply_mask = 0; /* bit n: reply a *cached* OPEN for file n */
static volatile int unlinked_mask = 0;     /* bit n: LOOKUP of file n fails ENOENT */
static unsigned long pas_open_replies, cached_open_replies;

static int fuse_fd = -1;
static unsigned char file_bytes[FILE_SIZE];
static volatile unsigned char *g_map;
static unsigned long read_requests, write_requests, other_requests, fsync_requests;
static int negotiated_minor = -1, negotiated_flags = -1;
static unsigned negotiated_flags2;
static unsigned negotiated_max_write, negotiated_max_stack_depth;
static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static int mounted;
static volatile int writer_stop, writer_mode;
static unsigned long writer_round;
static pthread_t writer_thread;
static const char *probe_argv1 = "(none)";

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

static int node_index(uint64_t nodeid) {
    if (nodeid < NODE_BASE || nodeid >= NODE_BASE + NFILES) return -1;
    return (int)(nodeid - NODE_BASE);
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
        switch (ih.opcode) {
        case FUSE_INIT: {
            const struct fuse_init_in *ii = (const void *)arg;
            struct un_fuse_init_out io;
            memset(&io, 0, sizeof(io));
            io.major = FUSE_KERNEL_VERSION;
            io.minor = ii->minor < 31 ? ii->minor : 31;
            uint64_t fl = (uint64_t)FUSE_BIG_WRITES | FUSE_ASYNC_READ | FUSE_MAX_PAGES |
                          UN_FUSE_INIT_EXT | UN_FUSE_PASSTHROUGH;
            io.flags = (uint32_t)(fl & 0xffffffffu);
            io.flags2 = (uint32_t)(fl >> 32);
            io.max_readahead = 0; io.max_write = MAX_IO;
            io.max_background = 16; io.congestion_threshold = 12;
            io.max_pages = NPAGES;
            io.max_stack_depth = 1; /* tmpfs backing has s_stack_depth 0 */
            negotiated_flags = (int)(fl & 0xffffffffu);
            negotiated_flags2 = io.flags2;
            negotiated_minor = io.minor;
            negotiated_max_write = io.max_write;
            negotiated_max_stack_depth = io.max_stack_depth;
            reply(ih.unique, 0, &io, sizeof(io));
            break;
        }
        case FUSE_LOOKUP: {
            const char *name = (const void *)arg;
            int idx = -1;
            for (int i = 0; i < NFILES; i++) if (strcmp(name, file_names[i]) == 0) idx = i;
            if (idx < 0 || (unlinked_mask & (1 << idx))) { reply(ih.unique, -ENOENT, NULL, 0); break; }
            struct fuse_entry_out eo;
            memset(&eo, 0, sizeof(eo));
            eo.nodeid = (uint64_t)(NODE_BASE + idx); eo.entry_valid = 1; eo.attr_valid = 1;
            fill_attr(&eo.attr, (uint64_t)(NODE_BASE + idx), S_IFREG | 0644, FILE_SIZE);
            reply(ih.unique, 0, &eo, sizeof(eo));
            break;
        }
        case FUSE_GETATTR: {
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            int idx = node_index(ih.nodeid);
            fill_attr(&ao.attr, ih.nodeid,
                      ih.nodeid == NODE_ROOT ? (S_IFDIR | 0755) : (S_IFREG | 0644),
                      ih.nodeid == NODE_ROOT ? 0 : FILE_SIZE);
            reply(ih.unique, 0, &ao, sizeof(ao));
            (void)idx;
            break;
        }
        case FUSE_OPEN: {
            int idx = node_index(ih.nodeid);
            struct un_fuse_open_out oo;
            memset(&oo, 0, sizeof(oo));
            oo.fh = 1;
            if (idx >= 0 && !(cached_reply_mask & (1 << idx)) && backing_id[idx] > 0) {
                oo.open_flags = UN_FOPEN_PASSTHROUGH;
                oo.backing_id = backing_id[idx];
                pas_open_replies++;
            } else {
                cached_open_replies++;
            }
            printf("server: OPEN nodeid=%llu -> open_flags=0x%x backing_id=%d (cached_replies=%lu "
                   "passthrough_replies=%lu)\n",
                   (unsigned long long)ih.nodeid, oo.open_flags, oo.backing_id, cached_open_replies,
                   pas_open_replies);
            reply(ih.unique, 0, &oo, sizeof(oo));
            break;
        }
        case FUSE_OPENDIR: {
            struct fuse_open_out oo;
            memset(&oo, 0, sizeof(oo));
            oo.fh = 2;
            reply(ih.unique, 0, &oo, sizeof(oo));
            break;
        }
        case FUSE_UNLINK: {
            const char *name = (const void *)arg;
            int idx = -1;
            for (int i = 0; i < NFILES; i++) if (strcmp(name, file_names[i]) == 0) idx = i;
            printf("server: UNLINK %s (idx=%d) -> 0\n", name, idx);
            if (idx >= 0) unlinked_mask |= (1 << idx);
            reply(ih.unique, 0, NULL, 0);
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
            if (wi->offset < FILE_SIZE) {
                uint32_t len = wi->size;
                if (len > FILE_SIZE - wi->offset) len = (uint32_t)(FILE_SIZE - wi->offset);
                memcpy(file_bytes + wi->offset, data, len);
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
            other_requests++;
            reply(ih.unique, -ENOSYS, NULL, 0);
            break;
        }
    }
    return NULL;
}

/* writer: ascending 8-byte words of page 0, one round = 512 stores + barriers */
static void *writer_main(void *unused) {
    (void)unused;
    unsigned long r = 0;
    while (!writer_stop) {
        r++;
        if (writer_mode == 1) {
            for (unsigned i = 0; i < WORDS_PER_PAGE; i++) {
                *(volatile unsigned long long *)(g_map + (size_t)i * 8) = r;
                __sync_synchronize();
            }
        } else if (writer_mode == 2) { /* slow control */
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
    writer_stop = 1; writer_mode = 0;
    CHK(pthread_join(writer_thread, NULL) == 0);
}

struct report {
    unsigned long n, breaks, breaks_in_page, breaks_at_page_end;
    unsigned long long max_delta, violations;
    unsigned long first_break_idx;
    unsigned long long min_val, max_val;
    unsigned distinct_values;
};
static unsigned long long inv_count;
static unsigned long long *inv_buf;
static void merge_count(unsigned long long *a, unsigned long long *t, unsigned lo, unsigned hi) {
    if (hi - lo < 2) return;
    unsigned mid = lo + (hi - lo) / 2;
    merge_count(a, t, lo, mid);
    merge_count(a, t, mid, hi);
    unsigned i = lo, j = mid, k = lo;
    while (i < mid && j < hi) {
        if (a[i] <= a[j]) t[k++] = a[i++];
        else { t[k++] = a[j++]; inv_count += (mid - i); }
    }
    while (i < mid) t[k++] = a[i++];
    while (j < hi) t[k++] = a[j++];
    for (unsigned x = lo; x < hi; x++) a[x] = t[x];
}
static int cmp_ull(const void *a, const void *b) {
    unsigned long long x = *(const unsigned long long *)a, y = *(const unsigned long long *)b;
    return x < y ? -1 : (x > y ? 1 : 0);
}
static struct report analyze_words(const unsigned char *buf, unsigned nwords) {
    static unsigned long long v[NWORDS], vals[NWORDS];
    static unsigned char present[NWORDS];
    for (unsigned i = 0; i < nwords; i++) {
        memcpy(&v[i], buf + (size_t)i * 8, 8);
        present[i] = 1;
    }
    struct report r;
    memset(&r, 0, sizeof(r));
    r.min_val = ~0ULL;
    r.first_break_idx = ~0UL;
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
            unsigned long long d = v[last] - v[i];
            if (d > r.max_delta) r.max_delta = d;
            if (r.first_break_idx == ~0UL) r.first_break_idx = i;
        }
        last = i;
    }
    if (r.min_val == ~0ULL) r.min_val = 0;
    if (nv) {
        qsort(vals, nv, sizeof(unsigned long long), cmp_ull);
        r.distinct_values = 1;
        for (unsigned i = 1; i < nv; i++) if (vals[i] != vals[i - 1]) r.distinct_values++;
    }
    /* inversion count over the sampled order (not the sorted copy) */
    static unsigned long long ordered[NWORDS];
    for (unsigned i = 0; i < nwords; i++) ordered[i] = v[i];
    inv_count = 0;
    if (nwords > 1) merge_count(ordered, inv_buf, 0, nwords);
    r.violations = inv_count;
    return r;
}

static int count_open_fds(void) {
    DIR *d = opendir("/proc/self/fd");
    if (!d) return -1;
    int n = 0;
    struct dirent *e;
    while ((e = readdir(d)) != NULL) if (e->d_name[0] != '.') n++;
    closedir(d);
    return n;
}

static int main_experiments(void) {
    fprintf(stderr, "probe: argv1=%s\n", probe_argv1);
    CHK(mkdir(MNT, 0755) == 0 || errno == EEXIST);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    if (fuse_fd < 0) { perror("open /dev/fuse"); return 2; }
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", fuse_fd, S_IFDIR,
             getuid(), getgid());
    if (mount("v1unpas", MNT, "fuse", MS_NOSUID | MS_NODEV, opts) != 0) { perror("mount"); return 2; }
    mounted = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t sthread;
    CHK(pthread_create(&sthread, NULL, server, NULL) == 0);
    msleep(50);

    int fds_before = count_open_fds(); /* before any backing file is opened */
    /* backing files on tmpfs (s_stack_depth 0 < max_stack_depth 1) */
    for (int i = 0; i < NFILES; i++) {
        char bpath[128];
        snprintf(bpath, sizeof(bpath), "/dev/shm/v1un-pas-%d", i);
        backing_fd[i] = open(bpath, O_RDWR | O_CREAT | O_TRUNC, 0644);
        if (backing_fd[i] < 0) { perror("open backing"); return 2; }
        if (ftruncate(backing_fd[i], FILE_SIZE) != 0) { perror("ftruncate backing"); return 2; }
    }

    /* register each backing file with the kernel */
    for (int i = 0; i < NFILES; i++) {
        struct un_fuse_backing_map m = {.fd = backing_fd[i], .flags = 0, .padding = 0};
        errno = 0;
        long rc = ioctl(fuse_fd, UN_FUSE_DEV_IOC_BACKING_OPEN, &m);
        int saved = errno;
        printf("PAS-0 BACKING_OPEN file=%s rc=%ld errno=%d (%s)\n", file_names[i], rc, saved,
               strerror(saved));
        if (rc <= 0) { printf("RESULT PAS0 passthrough_unavailable rc=%ld errno=%d\n", rc, saved); return 2; }
        backing_id[i] = (int)rc;
    }
    int fds_after_reg = count_open_fds();
    printf("probe: negotiated minor=%d flags=0x%x flags2=0x%x FUSE_PASSTHROUGH_reply_bit=%d "
           "max_stack_depth=%u max_write=%u\n",
           negotiated_minor, negotiated_flags, negotiated_flags2,
           (int)(((((uint64_t)negotiated_flags2) << 32) & UN_FUSE_PASSTHROUGH) != 0),
           negotiated_max_stack_depth, negotiated_max_write);
    CHK(negotiated_max_stack_depth == 1);

    /* ---------------- PAS-1 / the mapping + daemon read ------------------- */
    char path[256];
    snprintf(path, sizeof(path), "%s/%s", MNT, file_names[0]);
    int fd = open(path, O_RDWR);
    printf("PAS-1 client open %s rc=%d errno=%d\n", path, fd, errno);
    if (fd < 0) { printf("RESULT PAS1 passthrough_open_failed errno=%d\n", errno); return 3; }
    g_map = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    printf("PAS-1 mmap(MAP_SHARED|PROT_WRITE) rc=%s errno=%d\n",
           g_map == MAP_FAILED ? "MAP_FAILED" : "ok", errno);
    if (g_map == MAP_FAILED) { printf("RESULT PAS1 mmap_failed errno=%d\n", errno); return 3; }

    writer_start(1);
    msleep(60);
    unsigned char rbuf[FILE_SIZE];
    unsigned long rd0 = read_requests, wr0 = write_requests;
    ssize_t got1 = pread(backing_fd[0], rbuf, FILE_SIZE, 0);
    unsigned long long w0_first, w0_last;
    memcpy(&w0_first, rbuf, 8);
    memcpy(&w0_last, rbuf + (WORDS_PER_PAGE - 1) * 8, 8);
    msleep(20);
    ssize_t got2 = pread(backing_fd[0], rbuf, FILE_SIZE, 0);
    unsigned long long w1_first, w1_last;
    memcpy(&w1_first, rbuf, 8);
    memcpy(&w1_last, rbuf + (WORDS_PER_PAGE - 1) * 8, 8);
    unsigned long rd1 = read_requests, wr1 = write_requests;
    printf("PAS-1 daemon pread(backing_fd)=%zd/%zd words_seen page0=[%llu..%llu] then [%llu..%llu] "
           "fuse_read_requests_delta=%lu fuse_write_requests_delta=%lu\n",
           got1, got2, w0_first, w0_last, w1_first, w1_last, rd1 - rd0, wr1 - wr0);
    printf("RESULT PAS1 daemon_reads_mapped_dirty_bytes=%d without_client_cooperation=1 "
           "without_fsync=1 fuse_requests_for_that_read=%lu values_advanced=%d\n",
           got1 > 0 && w0_first > 0, (rd1 - rd0) + (wr1 - wr0), w1_first != w0_first);
    writer_stop_and_join();

    /* ---------------- PAS-2: is that read a single instant? --------------- */
    {
        unsigned trials = 0, torn = 0;
        unsigned long long max_delta = 0, max_viol = 0;
        writer_start(1);
        msleep(50);
        for (unsigned t = 0; t < 50; t++) {
            unsigned long long t0 = now_ns();
            pread(backing_fd[0], rbuf, FILE_SIZE, 0);
            unsigned long long dur = now_ns() - t0;
            struct report rep = analyze_words(rbuf, WORDS_PER_PAGE);
            trials++;
            if (rep.breaks || rep.violations) {
                torn++;
                if (rep.max_delta > max_delta) max_delta = rep.max_delta;
                if (rep.violations > max_viol) max_viol = rep.violations;
                if (torn <= 8)
                    printf("PAS-2 trial=%u pread_ns=%llu breaks=%lu in_page=%lu first_break_word=%lu "
                           "delta=%llu violating_pairs=%llu distinct_rounds=%u range=%llu..%llu\n",
                           t, dur, rep.breaks, rep.breaks_in_page, rep.first_break_idx,
                           rep.max_delta, rep.violations, rep.distinct_values, rep.min_val,
                           rep.max_val);
            }
        }
        printf("RESULT PAS2 daemon_read_trials=%u reads_that_are_NOT_a_single_instant=%u "
               "max_delta_rounds=%llu max_violating_pairs=%llu\n",
               trials, torn, max_delta, max_viol);

        /* PAS-2b: after the daemon's own fsync of the backing file */
        unsigned trials2 = 0, torn2 = 0;
        unsigned long long max_delta2 = 0;
        for (unsigned t = 0; t < 20; t++) {
            fsync(backing_fd[0]);
            pread(backing_fd[0], rbuf, FILE_SIZE, 0);
            struct report rep = analyze_words(rbuf, WORDS_PER_PAGE);
            trials2++;
            if (rep.breaks || rep.violations) {
                torn2++;
                if (rep.max_delta > max_delta2) max_delta2 = rep.max_delta;
            }
        }
        printf("RESULT PAS2b fsync_backing_then_read_trials=%u still_not_single_instant=%u "
               "max_delta_rounds=%llu\n",
               trials2, torn2, max_delta2);
        writer_stop_and_join();
    }

    /* ---------------- PAS-3: does the read stall the mapped writer? ------- */
    {
        writer_start(1);
        msleep(50);
        unsigned long r0 = writer_round;
        unsigned long long t0 = now_ns();
        for (unsigned t = 0; t < 50; t++) pread(backing_fd[0], rbuf, FILE_SIZE, 0);
        unsigned long long busy_ns = now_ns() - t0;
        unsigned long r1 = writer_round;
        msleep(200);
        unsigned long r2 = writer_round;
        unsigned long long idle_ns = 200ULL * 1000000ULL;
        unsigned long long busy_rounds = r1 - r0, idle_rounds = r2 - r1;
        printf("PAS-3 50x pread_128KiB took %llu ns; writer rounds during them=%llu; idle 200 ms "
               "yields %llu rounds\n",
               busy_ns, busy_rounds, idle_rounds);
        printf("RESULT PAS3 writer_rounds_per_ms_during_reads=%llu writer_rounds_per_ms_idle=%llu "
               "stall_ratio_pct=%llu\n",
               busy_ns ? busy_rounds * 1000000ULL / (busy_ns / 1000ULL) : 0,
               idle_rounds * 1000ULL / (idle_ns / 1000000ULL),
               idle_rounds ? (busy_rounds * 1000000ULL / (busy_ns / 1000ULL ? busy_ns / 1000ULL : 1)) *
                                 100ULL / (idle_rounds * 1000ULL / (idle_ns / 1000000ULL))
                           : 0);
        writer_stop_and_join();
    }

    /* ---------------- PAS-4: descriptor cost ------------------------------ */
    {
        int fds_now = count_open_fds();
        printf("PAS-4 daemon_fds baseline=%d after_opening_%d_backing_files=%d "
               "after_registering_them_with_the_kernel=%d during_%lu_passthrough_opens=%d "
               "(the kernel additionally allocates one backing_file per passthrough open, "
               "fs/fuse/passthrough.c:328-330, and one unbounded idr entry per registration)\n",
               fds_before, NFILES, fds_before + NFILES, fds_after_reg, pas_open_replies, fds_now);
        printf("RESULT PAS4 daemon_fd_delta_for_%d_backing_files=%d daemon_fds_per_passthrough_"
               "inode=%d client_passthrough_opens=%lu\n",
               NFILES, fds_after_reg - fds_before, fds_after_reg - fds_before, pas_open_replies);
    }

    /* ---------------- PAS-5: cached/passthrough exclusivity --------------- */
    {
        /* (a) cached handle first, then a passthrough open of the same inode */
        char p2[256];
        snprintf(p2, sizeof(p2), "%s/%s", MNT, file_names[1]);
        cached_reply_mask |= (1 << 1); /* server replies a *cached* OPEN for data2 */
        errno = 0;
        int h_cached = open(p2, O_RDWR);
        int e_hc = errno;
        cached_reply_mask &= ~(1 << 1); /* subsequent opens of data2 reply passthrough */
        printf("PAS-5a cached open %s rc=%d errno=%d\n", p2, h_cached, e_hc);
        errno = 0;
        int h_pas = open(p2, O_RDWR);
        int e_a = errno;
        printf("PAS-5a after a cached handle is open, passthrough open rc=%d errno=%d (%s)\n", h_pas,
               e_a, strerror(e_a));
        if (h_pas >= 0) close(h_pas);
        if (h_cached >= 0) close(h_cached);
        msleep(30);
        errno = 0;
        int h_pas2 = open(p2, O_RDWR);
        int e_a2 = errno;
        printf("PAS-5a after closing the cached handle, passthrough open rc=%d errno=%d (%s)\n",
               h_pas2, e_a2, strerror(e_a2));
        if (h_pas2 >= 0) close(h_pas2);
        printf("RESULT PAS5a passthrough_open_while_cached_handle_open_rc=%d errno=%d "
               "passthrough_open_after_close_rc=%d errno=%d\n",
               h_pas, e_a, h_pas2, e_a2);

        /* (b) passthrough handle first, then a cached open of the same inode */
        char p3[256];
        snprintf(p3, sizeof(p3), "%s/%s", MNT, file_names[2]);
        int pa = open(p3, O_RDWR | O_CREAT, 0644);
        printf("PAS-5b passthrough open %s rc=%d\n", p3, pa);
        cached_reply_mask |= (1 << 2); /* server now replies a cached OPEN for data3 */
        errno = 0;
        int hc = open(p3, O_RDWR);
        int e_b = errno;
        printf("PAS-5b while a passthrough handle is open, cached open rc=%d errno=%d (%s)\n", hc, e_b,
               strerror(e_b));
        cached_reply_mask &= ~(1 << 2);
        printf("RESULT PAS5b cached_open_while_passthrough_handle_open_rc=%d errno=%d\n", hc, e_b);
        if (hc >= 0) close(hc);
        if (pa >= 0) close(pa);
    }

    /* ---------------- PAS-6: open-unlinked mapping survival --------------- */
    {
        char p4[256];
        snprintf(p4, sizeof(p4), "%s/%s", MNT, file_names[3]);
        int h = open(p4, O_RDWR | O_CREAT, 0644);
        volatile unsigned char *m4 =
            mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, h, 0);
        printf("PAS-6 open+mmap %s rc=%d mmap=%s\n", p4, h, m4 == MAP_FAILED ? "FAILED" : "ok");
        if (m4 != MAP_FAILED) {
            *(volatile unsigned long long *)m4 = 0x1122334455667788ULL;
            __sync_synchronize();
            int urc = unlink(p4);
            printf("PAS-6 unlink rc=%d errno=%d\n", urc, errno);
            msleep(20);
            unsigned char b[8];
            pread(backing_fd[3], b, 8, 0);
            unsigned long long got;
            memcpy(&got, b, 8);
            *(volatile unsigned long long *)m4 = 0x99AABBCCDDEEFF00ULL;
            __sync_synchronize();
            msleep(20);
            unsigned char b2[8];
            pread(backing_fd[3], b2, 8, 0);
            unsigned long long got2;
            memcpy(&got2, b2, 8);
            errno = 0;
            int re = open(p4, O_RDWR);
            int e_re = errno;
            printf("PAS-6 after unlink: backing read 1 = 0x%llx, 2 = 0x%llx, reopen-by-path rc=%d "
                   "errno=%d\n",
                   got, got2, re, e_re);
            printf("RESULT PAS6 mapping_survives_unlink=%d daemon_still_reads_its_own_backing=%d "
                   "reopen_after_unlink_errno=%d\n",
                   got == 0x1122334455667788ULL && got2 == 0x99AABBCCDDEEFF00ULL,
                   got2 == 0x99AABBCCDDEEFF00ULL, e_re);
            if (re >= 0) close(re);
            munmap((void *)m4, FILE_SIZE);
        }
        if (h >= 0) close(h);
    }

    /* ---------------- PAS-7: FUSE request tallies ------------------------- */
    printf("RESULT PAS7 fuse_read_requests_total=%lu fuse_write_requests_total=%lu "
           "fuse_other_requests_total=%lu mapped_io_reached_the_daemon=%d\n",
           read_requests, write_requests, other_requests,
           (read_requests + write_requests) > 0);

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
    inv_buf = calloc(NWORDS, sizeof(unsigned long long));
    if (!inv_buf) return 2;
    return main_experiments();
}
