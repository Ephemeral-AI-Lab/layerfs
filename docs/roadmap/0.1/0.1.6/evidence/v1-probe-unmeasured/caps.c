/* v1-probe-unmeasured / CAPS probe.
 *
 * Purpose: establish, by measurement on the deployed kernel, what the FUSE
 * connection can negotiate and which kernel facilities exist at all:
 *
 *   CAPS-1  the exact FUSE_INIT request the deployed kernel sends (major, minor,
 *           max_readahead, flags, flags2) and the decoded bit names.
 *   CAPS-2  control: with a session that does NOT negotiate FUSE_PASSTHROUGH,
 *           FUSE_DEV_IOC_BACKING_OPEN on the daemon's /dev/fuse fd must fail
 *           (fuse_backing_open: `if (!fc->passthrough || !capable(CAP_SYS_ADMIN))
 *           res = -EPERM;`, fs/fuse/passthrough.c:224).
 *   CAPS-3  with a session that DOES negotiate FUSE_PASSTHROUGH
 *           (flags2 bit 5 = FUSE_PASSTHROUGH 1<<37, with FUSE_INIT_EXT set and
 *           max_stack_depth=1), the same ioctl must succeed and return a
 *           backing_id. That is the observable difference between "the kernel
 *           has the feature" and "the daemon cannot use it".
 *           -EOPNOTSUPP would mean CONFIG_FUSE_PASSTHROUGH is off
 *           (fs/fuse/dev.c:2411).
 *   CAPS-4  userfaultfd(2) availability on this kernel.
 *
 * No libfuse. One /dev/fuse fd per session, mounted with mount(2) under
 * CAP_SYS_ADMIN, exactly like the v1-probe-generic harness.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/fuse.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

/* ---- uapi 6.12 definitions missing from the container's 6.1 headers ----- */
#define UN_FUSE_INIT_EXT              (1u << 30)
#define UN_FUSE_PASSTHROUGH           (1ULL << 37)
#define UN_FUSE_DIRECT_IO_ALLOW_MMAP  (1ULL << 36)
#define UN_FUSE_HAS_INODE_DAX         (1ULL << 33)

struct un_fuse_init_out {
    uint32_t major, minor, max_readahead, flags;
    uint16_t max_background, congestion_threshold;
    uint32_t max_write, time_gran;
    uint16_t max_pages, map_alignment;
    uint32_t flags2;
    uint32_t max_stack_depth;
    uint32_t unused[6];
};

struct un_fuse_backing_map {
    int32_t fd;
    uint32_t flags;
    uint64_t padding;
};

#define UN_FUSE_DEV_IOC_MAGIC        229
#define UN_FUSE_DEV_IOC_BACKING_OPEN \
    _IOW(UN_FUSE_DEV_IOC_MAGIC, 1, struct un_fuse_backing_map)
#define UN_FUSE_DEV_IOC_BACKING_CLOSE \
    _IOW(UN_FUSE_DEV_IOC_MAGIC, 2, uint32_t)

struct session {
    const char *mnt;
    int negotiate_passthrough;
    unsigned max_stack_depth;
    int fd;
    volatile int stop;
    uint32_t seen_init_major, seen_init_minor, seen_init_flags, seen_init_flags2;
    uint32_t seen_max_readahead;
    int saw_init;
    /* what the session put in its FUSE_INIT reply */
    uint64_t reply_flags;
    pthread_t thread;
};

static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;

static int reply_fd(int fd, uint64_t unique, int error, const void *payload, size_t len) {
    unsigned char buf[sizeof(struct fuse_out_header) + 256];
    struct fuse_out_header oh = {.len = (uint32_t)(sizeof(oh) + len),
                                 .error = (int32_t)error,
                                 .unique = unique};
    if (sizeof(oh) + len > sizeof(buf)) return -1;
    memcpy(buf, &oh, sizeof(oh));
    if (len) memcpy(buf + sizeof(oh), payload, len);
    pthread_mutex_lock(&g_lock);
    ssize_t n = write(fd, buf, sizeof(oh) + len);
    pthread_mutex_unlock(&g_lock);
    return n == (ssize_t)(sizeof(oh) + len) ? 0 : -1;
}

/* The kernel rejects a read buffer smaller than
 * sizeof(fuse_in_header) + sizeof(fuse_write_in) + fc->max_write
 * (fs/fuse/dev.c fuse_dev_do_read, "-EINVAL"), so the daemon's read buffer must
 * have room for the negotiated max_write even when it only answers INIT. */
#define MAX_IO (1u << 20)

static void *server(void *arg) {
    struct session *s = arg;
    static unsigned char in[sizeof(struct fuse_in_header) + sizeof(struct fuse_write_in) + MAX_IO];
    while (!s->stop) {
        ssize_t n = read(s->fd, in, sizeof(in));
        if (n <= 0) {
            if (n < 0 && (errno == EINTR || errno == EAGAIN)) continue;
            return NULL;
        }
        if ((size_t)n < sizeof(struct fuse_in_header)) continue;
        struct fuse_in_header ih;
        memcpy(&ih, in, sizeof(ih));
        const unsigned char *a = in + sizeof(ih);
        if (ih.opcode == FUSE_INIT) {
            const struct fuse_init_in *ii = (const void *)a;
            struct un_fuse_init_out io;
            memset(&io, 0, sizeof(io));
            io.major = FUSE_KERNEL_VERSION;
            io.minor = ii->minor < 31 ? ii->minor : 31;
            io.max_readahead = 0;
            io.max_write = 1 << 20;
            io.max_background = 16;
            io.congestion_threshold = 12;
            io.max_pages = 32;
            uint64_t fl = (uint64_t)FUSE_BIG_WRITES | FUSE_ASYNC_READ | FUSE_MAX_PAGES;
            if (s->negotiate_passthrough) {
                fl |= UN_FUSE_INIT_EXT;         /* required for any flags2 bit */
                fl |= UN_FUSE_PASSTHROUGH;      /* -> flags2 bit 5 */
                io.max_stack_depth = s->max_stack_depth;
            }
            io.flags = (uint32_t)(fl & 0xffffffffu);
            io.flags2 = (uint32_t)(fl >> 32);
            pthread_mutex_lock(&g_lock);
            s->seen_init_major = ii->major;
            s->seen_init_minor = ii->minor;
            s->seen_max_readahead = ii->max_readahead;
            s->seen_init_flags = ii->flags;
            s->seen_init_flags2 = ii->flags2;
            s->seen_init_flags = ii->flags;
            s->saw_init = 1;
            s->reply_flags = fl;
            pthread_mutex_unlock(&g_lock);
            reply_fd(s->fd, ih.unique, 0, &io, sizeof(io));
            continue;
        }
        if (ih.opcode == FUSE_DESTROY) {
            reply_fd(s->fd, ih.unique, 0, NULL, 0);
            return NULL;
        }
        if (ih.opcode == FUSE_GETATTR || ih.opcode == FUSE_STATFS || ih.opcode == FUSE_OPENDIR ||
            ih.opcode == FUSE_ACCESS || ih.opcode == FUSE_FLUSH || ih.opcode == FUSE_RELEASE) {
            reply_fd(s->fd, ih.unique, -ENOSYS, NULL, 0);
            continue;
        }
        reply_fd(s->fd, ih.unique, -ENOSYS, NULL, 0);
    }
    return NULL;
}

static void decode_flags(uint32_t flags, uint32_t flags2) {
    uint64_t f = (uint64_t)flags | ((uint64_t)flags2 << 32);
    struct { uint64_t b; const char *n; } tab[] = {
        {FUSE_ASYNC_READ, "FUSE_ASYNC_READ"},
        {FUSE_BIG_WRITES, "FUSE_BIG_WRITES"},
        {FUSE_WRITEBACK_CACHE, "FUSE_WRITEBACK_CACHE"},
        {FUSE_MAX_PAGES, "FUSE_MAX_PAGES"},
        {FUSE_INIT_EXT, "FUSE_INIT_EXT"},
        {UN_FUSE_DIRECT_IO_ALLOW_MMAP, "FUSE_DIRECT_IO_ALLOW_MMAP"},
        {UN_FUSE_PASSTHROUGH, "FUSE_PASSTHROUGH"},
        {1ULL << 26, "FUSE_MAP_ALIGNMENT"},
        {UN_FUSE_HAS_INODE_DAX, "FUSE_HAS_INODE_DAX"},
        {1ULL << 27, "FUSE_SUBMOUNTS"},
        {1ULL << 39, "FUSE_HAS_RESEND"},
        {1ULL << 42, "FUSE_ALLOW_IDMAP"},
    };
    for (unsigned i = 0; i < sizeof(tab) / sizeof(tab[0]); i++)
        if (f & tab[i].b) printf("caps:   kernel-advertised bit %-30s (yes)\n", tab[i].n);
    for (unsigned i = 0; i < sizeof(tab) / sizeof(tab[0]); i++)
        if (!(f & tab[i].b)) printf("caps:   kernel-advertised bit %-30s (NO)\n", tab[i].n);
}

static int run_session(struct session *s) {
    mkdir(s->mnt, 0755);
    s->fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    if (s->fd < 0) { perror("open /dev/fuse"); return 2; }
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", s->fd, S_IFDIR,
             getuid(), getgid());
    if (mount("v1uncaps", s->mnt, "fuse", MS_NOSUID | MS_NODEV, opts) != 0) {
        perror("mount fuse"); return 2;
    }
    if (pthread_create(&s->thread, NULL, server, s) != 0) { perror("pthread_create"); return 2; }
    /* Force the INIT handshake: the first request through the mount waits for it. */
    struct stat st;
    (void)stat(s->mnt, &st);
    /* Touch the root so the kernel has a reason to complete INIT. */
    int rfd = open(s->mnt, O_RDONLY | O_DIRECTORY);
    if (rfd >= 0) close(rfd);
    for (int i = 0; i < 2000 && !s->saw_init; i++) usleep(1000);
    return 0;
}

static void end_session(struct session *s) {
    s->stop = 1;
    umount2(s->mnt, MNT_DETACH);
    pthread_join(s->thread, NULL);
    if (s->fd >= 0) close(s->fd);
}

int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("caps: env uid=%d euid=%d page_size=%ld\n", getuid(), geteuid(), sysconf(_SC_PAGESIZE));
    printf("caps: sizeof struct(un_fuse_init_out)=%zu (kernel expects 64)\n",
           sizeof(struct un_fuse_init_out));

    /* backing file for the ioctl: any regular file the daemon can open */
    int bfd = open("/dev/shm/v1un-caps-backing", O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (bfd < 0) { perror("open backing"); return 2; }
    if (ftruncate(bfd, 65536) != 0) { perror("ftruncate backing"); return 2; }

    /* ---------- session A: NO passthrough negotiated (control) ------------ */
    struct session A = {.mnt = "/mnt/v1uncaps-a", .negotiate_passthrough = 0, .fd = -1};
    if (run_session(&A) != 0) return 2;
    printf("caps: SESSION_A init_seen=%d major=%u minor=%u max_readahead=%u flags=0x%08x flags2=0x%08x\n",
           A.saw_init, A.seen_init_major, A.seen_init_minor, A.seen_max_readahead,
           A.seen_init_flags, A.seen_init_flags2);
    decode_flags(A.seen_init_flags, A.seen_init_flags2);
    printf("caps: SESSION_A reply_flags=0x%llx (passthrough not negotiated)\n",
           (unsigned long long)A.reply_flags);
    {
        struct un_fuse_backing_map m = {.fd = bfd, .flags = 0, .padding = 0};
        errno = 0;
        long rc = ioctl(A.fd, UN_FUSE_DEV_IOC_BACKING_OPEN, &m);
        printf("CAPS-2 control BACKING_OPEN_without_negotiated_passthrough rc=%ld errno=%d (%s)\n",
               rc, errno, strerror(errno));
        printf("RESULT CAPS2 control_rc=%ld control_errno=%d control_errno_name=%s\n", rc, errno,
               errno == EPERM ? "EPERM" : errno == EOPNOTSUPP ? "EOPNOTSUPP" : "other");
    }
    end_session(&A);

    /* ---------- session B: passthrough negotiated ------------------------- */
    struct session B = {.mnt = "/mnt/v1uncaps-b", .negotiate_passthrough = 1,
                        .max_stack_depth = 1, .fd = -1};
    if (run_session(&B) != 0) return 2;
    printf("caps: SESSION_B init_seen=%d major=%u minor=%u flags=0x%08x flags2=0x%08x\n",
           B.saw_init, B.seen_init_major, B.seen_init_minor, B.seen_init_flags, B.seen_init_flags2);
    printf("caps: SESSION_B reply_flags=0x%llx FUSE_INIT_EXT=%d FUSE_PASSTHROUGH=%d "
           "max_stack_depth=%u\n",
           (unsigned long long)B.reply_flags,
           (int)((B.reply_flags & UN_FUSE_INIT_EXT) != 0),
           (int)((B.reply_flags & UN_FUSE_PASSTHROUGH) != 0), B.max_stack_depth);
    {
        struct un_fuse_backing_map m = {.fd = bfd, .flags = 0, .padding = 0};
        errno = 0;
        long rc = ioctl(B.fd, UN_FUSE_DEV_IOC_BACKING_OPEN, &m);
        int saved = errno;
        printf("CAPS-3 BACKING_OPEN_with_negotiated_passthrough rc=%ld errno=%d (%s)\n", rc, saved,
               strerror(saved));
        if (rc > 0) {
            uint32_t id = (uint32_t)rc;
            errno = 0;
            long crc = ioctl(B.fd, UN_FUSE_DEV_IOC_BACKING_CLOSE, &id);
            printf("CAPS-3 BACKING_CLOSE id=%u rc=%ld errno=%d\n", id, crc, errno);
        }
        printf("RESULT CAPS3 backing_id=%ld rc_errno=%d verdict=%s\n", rc, saved,
               rc > 0 ? "PASSTHROUGH_AVAILABLE_AND_NEGOTIABLE"
                      : (saved == EOPNOTSUPP ? "CONFIG_FUSE_PASSTHROUGH_OFF"
                                             : "NEGOTIATION_FAILED"));
    }
    end_session(&B);

    /* ---------- userfaultfd ------------------------------------------------- */
    {
        errno = 0;
        long ufd = syscall(SYS_userfaultfd, O_CLOEXEC | O_NONBLOCK);
        int saved = errno;
        printf("CAPS-4 userfaultfd(2) rc=%ld errno=%d (%s)\n", ufd, saved, strerror(saved));
        if (ufd >= 0) close((int)ufd);
        printf("RESULT CAPS4 userfaultfd_available=%d errno_name=%s\n", ufd >= 0,
               saved == ENOSYS ? "ENOSYS" : saved == EPERM ? "EPERM" : "other");
    }

    close(bfd);
    printf("caps: done\n");
    return 0;
}
