/* v1-probe-generic: bounded mechanism probe for the V1 question.
 * Question: can a generic, in-tree-supported FUSE implementation obtain, at a
 * chosen instant, a copy of the bytes that writable shared mappings of a file
 * currently expose to a reader -- such that the copy is stable (later mapped
 * writes do not change it) -- without a global freeze?
 *
 * Serves one cached FUSE session over /dev/fuse (no libfuse), mounts it with
 * mount(2) under CAP_SYS_ADMIN, mmaps served files MAP_SHARED/PROT_WRITE and
 * measures: T1 delivered copy vs alias, T2 notification-return instant, T3 one
 * reply vs point-in-time image, T4 sequential collection, T5 daemon read(2),
 * T6 daemon-initiated per-file drain.
 *
 * Concurrency oracle for T3/T4/T5/T6: a writer thread does single aligned
 * 8-byte stores separated by __sync_synchronize() (DMB on aarch64), one value
 * per round, in fixed order:
 *   mode PAGES: for p in 0..NPAGES-1 ascending: store round r at page p
 *   mode TWO  : store round r at file A page 0, then file B page 0
 * Reachable states therefore always satisfy value(page p) >= value(page q) for
 * all p < q at every instant (one round completes before the next begins), so
 * any collected sample set that violates that ordering mixes two instants.
 * The oracle assumes nothing about copy or delivery order.
 *
 * argv[1] == "writeback" adds FUSE_WRITEBACK_CACHE.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/resource.h>
#include <linux/fuse.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define PAGE_SZ 4096u
#define NPAGES 32
#define FILE_SIZE (NPAGES * PAGE_SZ)
/* A larger file for the whole-range drain test (T9): 4 MiB = 1024 pages, i.e.
 * 32 writeback requests at the negotiated 128 KiB max_pages. */
#define BIG_PAGES 1024u
#define FILE_BIG_SIZE (BIG_PAGES * PAGE_SZ)
#define DRAIN_MAX BIG_PAGES
#define MAX_IO (1u << 20)
#define MNT "/mnt/v1generic"
#define NODE_ROOT 1
#define NODE_FILE 2
#define NODE_FILE2 3
#define NODE_BIG 4

static int fuse_fd = -1;
static int writeback_mode = 0;
static unsigned char file_bytes[FILE_SIZE];
static unsigned char file2_bytes[FILE_SIZE];
static unsigned char big_bytes[FILE_BIG_SIZE];
static unsigned g_writer_npages = NPAGES;
static unsigned g_drain_npages = NPAGES;
static unsigned long write_requests, write_bytes, write_cache_requests;
static unsigned long read_requests, fsync_requests, other_requests;
static unsigned char retrieved[MAX_IO];
static uint32_t retrieved_size;
static uint64_t retrieved_cookie;
static unsigned long long drain_val[DRAIN_MAX];
static unsigned char drain_seen[DRAIN_MAX];
/* per-node (indexed by nodeid) copies of the last drained value per page, so
 * the two-file collection test can compare file A's and file B's payloads. */
static unsigned long long drain_val_n[8][DRAIN_MAX];
static unsigned char drain_seen_n[8][DRAIN_MAX];
static unsigned long drain_writes, drain_write_cache, drain_pages;
static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static int negotiated_minor = -1, negotiated_flags = -1;
static unsigned int negotiated_max_write;
static volatile int server_park, server_parked;
static volatile int writer_mode, writer_stop;
static volatile unsigned long writer_round;
static volatile unsigned char *map0, *map1;
static pthread_t writer_thread;

static void msleep(int ms) {
    struct timespec ts = {ms / 1000, (long)(ms % 1000) * 1000000L};
    nanosleep(&ts, NULL);
}

/* Never abort()/exit() with the FUSE mount live: that wedged a container once.
 * Every failure path detaches the mount and _exit()s. */
static volatile int mounted;
static const char *probe_argv1 = "(none)";

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
    write(2, "probe: WATCHDOG timeout, detaching mount and exiting\n", 52);
    teardown_and_exit(9);
}

#define CHK(cond) do { if (!(cond)) die("check failed: " #cond, 7); } while (0)

static int reply(uint64_t unique, int error, const void *payload, size_t len) {
    static unsigned char buf[sizeof(struct fuse_out_header) + MAX_IO];
    struct fuse_out_header oh;
    size_t total = sizeof(oh) + len;
    if (total > sizeof(buf)) return -1;
    oh.len = (uint32_t)total;
    oh.error = (int32_t)error;
    oh.unique = unique;
    pthread_mutex_lock(&g_lock);
    memcpy(buf, &oh, sizeof(oh));
    if (len) memcpy(buf + sizeof(oh), payload, len);
    ssize_t n = write(fuse_fd, buf, total);
    pthread_mutex_unlock(&g_lock);
    return n == (ssize_t)total ? 0 : -1;
}

static void fill_attr(struct fuse_attr *attr, uint64_t ino, uint32_t mode, uint64_t size) {
    memset(attr, 0, sizeof(*attr));
    attr->ino = ino; attr->size = size; attr->blocks = (size + 511) / 512;
    attr->mode = mode; attr->nlink = (mode & S_IFDIR) ? 2 : 1;
    attr->uid = 0; attr->gid = 0; attr->blksize = PAGE_SZ;
}

static unsigned char *backing(uint64_t n) {
    if (n == NODE_FILE2) return file2_bytes;
    if (n == NODE_BIG) return big_bytes;
    return file_bytes;
}

/* First 8 bytes of page p inside a write payload, or -1 if not fully covered. */
static long long payload_word(const unsigned char *data, uint64_t off, uint32_t size, unsigned p) {
    uint64_t ps = (uint64_t)p * PAGE_SZ;
    if (off > ps || off + size < ps + 8) return -1;
    unsigned long long v;
    memcpy(&v, data + (ps - off), sizeof(v));
    return (long long)v;
}

static void *server(void *unused) {
    (void)unused;
    static unsigned char in[sizeof(struct fuse_in_header) + sizeof(struct fuse_write_in) + MAX_IO]
        __attribute__((aligned(8)));
    for (;;) {
        /* poll instead of blocking in read(2) so that server_park is re-checked
         * even while a request is already queued (T2 needs the references held
         * by a queued notification with no page content copied yet). */
        if (server_park) {
            struct pollfd p = {.fd = fuse_fd, .events = POLLIN};
            poll(&p, 1, 5);
            server_parked = 1;
            continue;
        }
        server_parked = 0;
        ssize_t n;
        {
            struct pollfd p = {.fd = fuse_fd, .events = POLLIN};
            poll(&p, 1, 5);
            if (!(p.revents & POLLIN)) continue;
            n = read(fuse_fd, in, sizeof(in));
        }
        if (n <= 0) {
            if (errno == EINTR || errno == EAGAIN) continue;
            fprintf(stderr, "probe: server read returned %zd errno=%d (%s)\n", n, errno,
                    strerror(errno));
            break;
        }
        struct fuse_in_header ih;
        if ((size_t)n < sizeof(ih)) continue;
        memcpy(&ih, in, sizeof(ih));
        const unsigned char *arg = in + sizeof(ih);
        size_t arglen = (size_t)n - sizeof(ih);
        if (ih.opcode == FUSE_NOTIFY_REPLY) {
            const struct fuse_notify_retrieve_in *ri = (const void *)arg;
            size_t hdr = sizeof(*ri);
            if (arglen < hdr || arglen != hdr + ri->size || ri->size > sizeof(retrieved)) {
                fprintf(stderr, "probe: FATAL notify reply arglen=%zu size=%u\n", arglen,
                        arglen >= hdr ? ri->size : 0);
                die("bad notify reply", 4);
            }
            pthread_mutex_lock(&g_lock);
            memcpy(retrieved, arg + hdr, ri->size);
            retrieved_size = ri->size;
            retrieved_cookie = ih.unique;
            pthread_mutex_unlock(&g_lock);
            continue; /* FUSE_NOTIFY_REPLY expects no daemon reply */
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
            uint64_t nodeid;
            if (strcmp(name, "data") == 0) nodeid = NODE_FILE;
            else if (strcmp(name, "data2") == 0) nodeid = NODE_FILE2;
            else if (strcmp(name, "big") == 0) nodeid = NODE_BIG;
            else { reply(ih.unique, -ENOENT, NULL, 0); break; }
            struct fuse_entry_out eo;
            memset(&eo, 0, sizeof(eo));
            eo.nodeid = nodeid; eo.entry_valid = 1; eo.attr_valid = 1;
            fill_attr(&eo.attr, nodeid, S_IFREG | 0644,
                      nodeid == NODE_BIG ? FILE_BIG_SIZE : FILE_SIZE);
            reply(ih.unique, 0, &eo, sizeof(eo));
            break;
        }
        case FUSE_GETATTR: {
            uint64_t ino = ih.nodeid;
            if (ino != NODE_ROOT && ino != NODE_FILE && ino != NODE_FILE2 && ino != NODE_BIG)
                ino = NODE_FILE;
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            fill_attr(&ao.attr, ino, ino == NODE_ROOT ? (S_IFDIR | 0755) : (S_IFREG | 0644),
                      ino == NODE_ROOT ? 0 : (ino == NODE_BIG ? FILE_BIG_SIZE : FILE_SIZE));
            reply(ih.unique, 0, &ao, sizeof(ao));
            break;
        }
        case FUSE_OPEN: {
            struct fuse_open_out oo;
            memset(&oo, 0, sizeof(oo));
            oo.fh = 1; oo.open_flags = 0; /* page cache enabled, no direct I/O */
            reply(ih.unique, 0, &oo, sizeof(oo));
            break;
        }
        case FUSE_READ: {
            const struct fuse_read_in *ri = (const void *)arg;
            uint64_t off = ri->offset;
            uint32_t want = ri->size;
            read_requests++;
            unsigned long long fsz = (ih.nodeid == NODE_BIG) ? FILE_BIG_SIZE : FILE_SIZE;
            if (off >= fsz) { reply(ih.unique, 0, NULL, 0); break; }
            if (want > fsz - off) want = (uint32_t)(fsz - off);
            reply(ih.unique, 0, backing(ih.nodeid) + off, want);
            break;
        }
        case FUSE_WRITE: {
            const struct fuse_write_in *wi = (const void *)arg;
            const unsigned char *data = arg + sizeof(*wi);
            uint64_t off = wi->offset;
            uint32_t size = wi->size;
            write_requests++; write_bytes += size;
            if (wi->write_flags & FUSE_WRITE_CACHE) write_cache_requests++;
            unsigned long long fsz = (ih.nodeid == NODE_BIG) ? FILE_BIG_SIZE : FILE_SIZE;
            if (off < fsz) {
                uint32_t len = size;
                if (len > fsz - off) len = (uint32_t)(fsz - off);
                memcpy(backing(ih.nodeid) + off, data, len);
            }
            drain_writes++;
            if (wi->write_flags & FUSE_WRITE_CACHE) drain_write_cache++;
            for (unsigned p = 0; p < g_drain_npages; p++) {
                long long v = payload_word(data, off, size, p);
                if (v >= 0) {
                    if (!drain_seen[p]) drain_pages++;
                    drain_seen[p] = 1;
                    drain_val[p] = (unsigned long long)v;
                    unsigned ni = (unsigned)(ih.nodeid & 7u);
                    drain_seen_n[ni][p] = 1;
                    drain_val_n[ni][p] = (unsigned long long)v;
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
        case FUSE_OPENDIR: {
            struct fuse_open_out oo;
            memset(&oo, 0, sizeof(oo));
            oo.fh = 2;
            reply(ih.unique, 0, &oo, sizeof(oo));
            break;
        }
        case FUSE_SETATTR: {
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            fill_attr(&ao.attr, ih.nodeid, S_IFREG | 0644, FILE_SIZE);
            reply(ih.unique, 0, &ao, sizeof(ao));
            break;
        }
        case FUSE_FLUSH:
        case FUSE_RELEASE:
        case FUSE_RELEASEDIR:
        case FUSE_ACCESS:
            other_requests++;
            reply(ih.unique, 0, NULL, 0);
            break;
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

static inline void w64(volatile unsigned char *m, size_t off, unsigned long v) {
    *(volatile unsigned long long *)(m + off) = (unsigned long long)v;
    __sync_synchronize();
}

static void *writer_main(void *unused) {
    (void)unused;
    unsigned long r = 0;
    while (!writer_stop) {
        r++;
        int mode = writer_mode;
        if (mode == 1) {
            /* ascending page order: at every instant value(page p) >=
             * value(page q) for p < q, so a sample with v[p] < v[q] mixes
             * two instants (check_ordered flags exactly that). */
            for (unsigned p = 0; p < g_writer_npages; p++) w64(map0, (size_t)p * PAGE_SZ, r);
        } else if (mode == 2) {
            w64(map0, 0, r);
            w64(map1, 0, r);
        } else {
            msleep(1);
        }
        writer_round = r;
    }
    return NULL;
}

static void writer_start(int mode) {
    writer_stop = 0; writer_round = 0; writer_mode = mode;
    assert(pthread_create(&writer_thread, NULL, writer_main, NULL) == 0);
}

static void writer_stop_and_join(void) {
    writer_stop = 1; writer_mode = 0;
    assert(pthread_join(writer_thread, NULL) == 0);
}

static uint64_t next_cookie = 100;

static void retrieve(uint64_t nodeid, uint64_t cookie, uint64_t offset, uint32_t size) {
    struct fuse_notify_retrieve_out r;
    memset(&r, 0, sizeof(r));
    r.notify_unique = cookie; r.nodeid = nodeid; r.offset = offset; r.size = size;
    assert(reply(0, FUSE_NOTIFY_RETRIEVE, &r, sizeof(r)) == 0);
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

struct inv { unsigned long pairs, viol; unsigned long long max_delta, min_delta; unsigned npages; };

static struct inv check_ordered(const unsigned long long *v, const unsigned char *present,
                                unsigned npages) {
    struct inv r;
    memset(&r, 0, sizeof(r));
    r.min_delta = ~0ULL;
    for (unsigned p = 0; p < npages; p++) {
        if (!present[p]) continue;
        r.npages++;
        for (unsigned q = p + 1; q < npages; q++) {
            if (!present[q]) continue;
            r.pairs++;
            if (v[p] == v[q]) continue;
            unsigned long long d = v[p] > v[q] ? v[p] - v[q] : v[q] - v[p];
            if (v[p] < v[q]) r.viol++;
            if (d > r.max_delta) r.max_delta = d;
            if (d < r.min_delta) r.min_delta = d;
        }
    }
    if (r.min_delta == ~0ULL) r.min_delta = 0;
    return r;
}

static unsigned npages_from_retrieved(unsigned long long *v, unsigned char *present) {
    unsigned n = retrieved_size / PAGE_SZ;
    if (n > NPAGES) n = NPAGES;
    for (unsigned p = 0; p < NPAGES; p++) {
        present[p] = 0; v[p] = 0;
        if (p < n) { memcpy(&v[p], retrieved + (size_t)p * PAGE_SZ, sizeof(v[p])); present[p] = 1; }
    }
    return n;
}

/* Last: keep the mount live only while experiments run. */
#include "probe_experiments.h"
