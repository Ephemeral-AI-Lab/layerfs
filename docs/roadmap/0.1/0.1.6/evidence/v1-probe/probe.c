/* Minimal V1 hypothesis probe: is a MAP_SHARED-dirtied page on a cached
 * (write-through, no FUSE_WRITEBACK_CACHE) FUSE file visible to the FUSE
 * daemon before kernel writeback?
 *
 * One process: a FUSE server thread serves /data from a private buffer and
 * counts WRITE requests; the main thread mounts it, mmaps it MAP_SHARED,
 * stores a byte, and reports whether the daemon observed it.
 *
 * No libfuse dependency: the session is served over /dev/fuse and mounted with
 * mount(2) under CAP_SYS_ADMIN, exactly like libfuse's direct-mount path.
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
#include <sys/mman.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define FILE_SIZE 4096
#define NODE_FILE 2
#define NODE_FILE2 3
#define NODE_ROOT 1

static int fuse_fd = -1;
static unsigned char file_bytes[FILE_SIZE];
static unsigned char file2_bytes[FILE_SIZE];
static volatile unsigned long write_requests = 0;
static volatile unsigned long write_bytes = 0;
static volatile int server_stop = 0;
static volatile int unlinked2 = 0;
static int negotiated_flags = -1;
static unsigned int negotiated_minor = 0;
static unsigned int negotiated_max_write = 0;

static void msleep(int ms) {
    struct timespec ts = {ms / 1000, (long)(ms % 1000) * 1000000L};
    nanosleep(&ts, NULL);
}

static int reply(uint64_t unique, int error, const void *payload, size_t len) {
    unsigned char buf[8192];
    struct fuse_out_header oh;
    size_t total = sizeof(oh) + len;
    if (total > sizeof(buf)) return -1;
    oh.len = (uint32_t)total;
    oh.error = error;
    oh.unique = unique;
    memcpy(buf, &oh, sizeof(oh));
    if (len) memcpy(buf + sizeof(oh), payload, len);
    return write(fuse_fd, buf, total) == (ssize_t)total ? 0 : -1;
}

static void fill_attr(struct fuse_attr *attr, uint64_t ino, uint32_t mode, uint64_t size) {
    memset(attr, 0, sizeof(*attr));
    attr->ino = ino;
    attr->size = size;
    attr->blocks = (size + 511) / 512;
    attr->mode = mode;
    attr->nlink = (mode & S_IFDIR) ? 2 : 1;
    attr->uid = 0;
    attr->gid = 0;
    attr->blksize = 4096;
}

static void *server(void *unused) {
    (void)unused;
    /* >= in_header + write_in + negotiated max_write (kernel read rule) */
    static unsigned char in[(1 << 20) + 8192] __attribute__((aligned(8)));
    for (;;) {
        ssize_t n = read(fuse_fd, in, sizeof(in));
        if (n <= 0) {
            if (errno == EINTR || errno == EAGAIN) continue;
            fprintf(stderr, "probe: server read returned %zd errno=%d (%s)\n", n, errno, strerror(errno));
            break;
        }
        struct fuse_in_header ih;
        if ((size_t)n < sizeof(ih)) continue;
        memcpy(&ih, in, sizeof(ih));
        const void *arg = in + sizeof(ih);
        if (server_stop && ih.opcode != FUSE_FORGET && ih.opcode != FUSE_INTERRUPT) {
            /* keep serving until destroy/unmount */
        }
        switch (ih.opcode) {
        case FUSE_INIT: {
            const struct fuse_init_in *ii = arg;
            struct fuse_init_out io;
            memset(&io, 0, sizeof(io));
            io.major = FUSE_KERNEL_VERSION;
            io.minor = ii->minor < 31 ? ii->minor : 31;
            /* Cached mode, write-through: deliberately NOT FUSE_WRITEBACK_CACHE
             * and not FUSE_DIRECT_IO_ALLOW_MMAP. */
            io.flags = FUSE_BIG_WRITES | FUSE_ASYNC_READ;
            io.max_readahead = 0;
            io.max_write = 1 << 20;
            io.max_background = 16;
            io.congestion_threshold = 12;
            negotiated_flags = (int)io.flags;
            negotiated_minor = io.minor;
            negotiated_max_write = io.max_write;
            reply(ih.unique, 0, &io, sizeof(io));
            break;
        }
        case FUSE_LOOKUP: {
            const char *name = arg;
            uint64_t nodeid;
            if (strcmp(name, "data") == 0) {
                nodeid = NODE_FILE;
            } else if (strcmp(name, "data2") == 0) {
                if (unlinked2) {
                    reply(ih.unique, -ENOENT, NULL, 0);
                    break;
                }
                nodeid = NODE_FILE2;
            } else {
                reply(ih.unique, -ENOENT, NULL, 0);
                break;
            }
            struct fuse_entry_out eo;
            memset(&eo, 0, sizeof(eo));
            eo.nodeid = nodeid;
            eo.generation = 0;
            eo.entry_valid = 1;
            eo.attr_valid = 1;
            fill_attr(&eo.attr, nodeid, S_IFREG | 0644, FILE_SIZE);
            reply(ih.unique, 0, &eo, sizeof(eo));
            break;
        }
        case FUSE_GETATTR: {
            const struct fuse_getattr_in *gi = arg;
            uint64_t ino = (gi->getattr_flags & FUSE_GETATTR_FH) ? ih.nodeid : ih.nodeid;
            if (ino != NODE_ROOT && ino != NODE_FILE && ino != NODE_FILE2) ino = NODE_FILE;
            struct fuse_attr_out ao;
            memset(&ao, 0, sizeof(ao));
            ao.attr_valid = 1;
            fill_attr(&ao.attr, ino, ino == NODE_ROOT ? (S_IFDIR | 0755) : (S_IFREG | 0644),
                      ino == NODE_ROOT ? 0 : FILE_SIZE);
            reply(ih.unique, 0, &ao, sizeof(ao));
            break;
        }
        case FUSE_OPEN:
            {
                struct fuse_open_out oo;
                memset(&oo, 0, sizeof(oo));
                oo.fh = 1;
                oo.open_flags = 0; /* kernel page cache enabled, no direct I/O */
                reply(ih.unique, 0, &oo, sizeof(oo));
            }
            break;
        case FUSE_READ: {
            const struct fuse_read_in *ri = arg;
            unsigned char data[FILE_SIZE];
            uint64_t off = ri->offset;
            uint32_t want = ri->size;
            if (off >= FILE_SIZE) {
                reply(ih.unique, 0, NULL, 0);
                break;
            }
            if (want > FILE_SIZE - off) want = (uint32_t)(FILE_SIZE - off);
            memcpy(data, (ih.nodeid == NODE_FILE2 ? file2_bytes : file_bytes) + off, want);
            reply(ih.unique, 0, data, want);
            break;
        }
        case FUSE_WRITE: {
            const struct fuse_write_in *wi = arg;
            const unsigned char *data = (const unsigned char *)arg + sizeof(*wi);
            if (wi->offset < FILE_SIZE) {
                uint32_t len = wi->size;
                if (len > FILE_SIZE - wi->offset) len = (uint32_t)(FILE_SIZE - wi->offset);
                memcpy((ih.nodeid == NODE_FILE2 ? file2_bytes : file_bytes) + wi->offset,
                       data, len);
            }
            __sync_synchronize();
            write_bytes += wi->size;
            write_requests++;
            struct fuse_write_out wo;
            memset(&wo, 0, sizeof(wo));
            wo.size = wi->size;
            reply(ih.unique, 0, &wo, sizeof(wo));
            break;
        }
        case FUSE_UNLINK: {
            const char *name = arg;
            if (strcmp(name, "data2") == 0) {
                unlinked2 = 1;
                reply(ih.unique, 0, NULL, 0);
            } else {
                reply(ih.unique, -ENOENT, NULL, 0);
            }
            break;
        }
        case FUSE_FLUSH:
        case FUSE_FSYNC:
        case FUSE_FSYNCDIR:
        case FUSE_ACCESS:
        case FUSE_RELEASE:
        case FUSE_RELEASEDIR:
        case FUSE_SETATTR:
        case FUSE_OPENDIR:
            if (ih.opcode == FUSE_OPENDIR) {
                struct fuse_open_out oo;
                memset(&oo, 0, sizeof(oo));
                oo.fh = 2;
                reply(ih.unique, 0, &oo, sizeof(oo));
            } else if (ih.opcode == FUSE_SETATTR) {
                struct fuse_attr_out ao;
                memset(&ao, 0, sizeof(ao));
                ao.attr_valid = 1;
                fill_attr(&ao.attr, ih.nodeid, S_IFREG | 0644, FILE_SIZE);
                reply(ih.unique, 0, &ao, sizeof(ao));
            } else {
                reply(ih.unique, 0, NULL, 0);
            }
            break;
        case FUSE_STATFS: {
            struct fuse_statfs_out so;
            memset(&so, 0, sizeof(so));
            so.st.bsize = 4096;
            so.st.blocks = 1024;
            so.st.bfree = 512;
            so.st.bavail = 512;
            so.st.files = 16;
            so.st.ffree = 8;
            so.st.namelen = 255;
            reply(ih.unique, 0, &so, sizeof(so));
            break;
        }
        case FUSE_GETXATTR:
        case FUSE_LISTXATTR:
            reply(ih.unique, -ENOSYS, NULL, 0);
            break;
        case FUSE_FORGET:
        case FUSE_INTERRUPT:
            break;
        case FUSE_DESTROY:
            server_stop = 1;
            reply(ih.unique, 0, NULL, 0);
            return NULL;
        default:
            reply(ih.unique, -ENOSYS, NULL, 0);
            break;
        }
    }
    return NULL;
}

static int wait_for_writes(unsigned long target, int budget_ms, const char *label) {
    for (int waited = 0; waited < budget_ms; waited += 10) {
        if (write_requests >= target) return waited;
        msleep(10);
    }
    fprintf(stderr, "probe: %s still had %lu write requests after %d ms\n", label,
            write_requests, budget_ms);
    return -1;
}

int main(void) {
    const char *mnt = "/mnt/v1probe";
    mkdir(mnt, 0755);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    if (fuse_fd < 0) {
        perror("open /dev/fuse");
        return 2;
    }
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", fuse_fd, S_IFDIR,
             getuid(), getgid());
    if (mount("v1probe", mnt, "fuse", MS_NOSUID | MS_NODEV, opts) != 0) {
        perror("mount fuse");
        return 2;
    }
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t thread;
    int started = pthread_create(&thread, NULL, server, NULL);
    if (started != 0) { fprintf(stderr, "pthread_create failed: %d\n", started); return 2; }

    printf("probe: mounted write-through cached FUSE session\n");
    fflush(stdout);

    char path[256];
    snprintf(path, sizeof(path), "%s/data", mnt);
    int fd = open(path, O_RDWR);
    if (fd < 0) {
        perror("open probe file");
        return 3;
    }
    volatile unsigned char *mapping =
        mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapping == MAP_FAILED) {
        perror("mmap MAP_SHARED");
        return 3;
    }
    printf("probe: negotiated minor=%u flags=0x%x max_write=%u\n", negotiated_minor,
           negotiated_flags, negotiated_max_write);
    fflush(stdout);

    mapping[0] = 'A';
    __sync_synchronize();
    int observed = wait_for_writes(1, 1000, "post-store");
    printf("probe: after-map-store daemon_write_requests=%lu daemon_bytes=%lu "
           "daemon_saw_first_byte=%d observed_after_ms=%d\n",
           write_requests, write_bytes, file_bytes[0] == (unsigned char)'A' ? 1 : 0, observed);
    fflush(stdout);

    unsigned long before = write_requests;
    int dirfd = open(mnt, O_RDONLY | O_DIRECTORY);
    if (dirfd < 0) {
        perror("open mount root");
        return 4;
    }
    if (syncfs(dirfd) != 0) perror("syncfs");
    int after_sync = wait_for_writes(before + 1, 5000, "post-syncfs");
    printf("probe: after-syncfs daemon_write_requests=%lu daemon_first_byte=%u "
           "observed_after_ms=%d\n",
           write_requests, file_bytes[0], after_sync);
    fflush(stdout);

    /* Second phase: a fresh mapping store after the drain is again invisible
     * until the next writeback trigger. */
    before = write_requests;
    mapping[1] = 'B';
    __sync_synchronize();
    int second = wait_for_writes(before + 1, 500, "post-second-store");
    printf("probe: after-second-store daemon_saw_second_byte=%d observed_after_ms=%d\n",
           file_bytes[1] == (unsigned char)'B' ? 1 : 0, second);
    fflush(stdout);

    /* Phase 3 (candidate mechanism M2): the daemon flushes one file that has a
     * writable mapping by opening it through its own mount and calling fsync;
     * no global operation barrier is taken. */
    char path2[256];
    snprintf(path2, sizeof(path2), "%s/data2", mnt);
    int fd2 = open(path2, O_RDWR);
    if (fd2 < 0) {
        perror("open data2");
        return 5;
    }
    volatile unsigned char *mapping2 =
        mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd2, 0);
    if (mapping2 == MAP_FAILED) {
        perror("mmap data2");
        return 5;
    }
    mapping2[0] = 'C';
    __sync_synchronize();
    int third = wait_for_writes(before + 1, 300, "post-third-store");
    printf("probe: per-file self-open pre-fsync daemon_write_requests=%lu "
           "daemon_saw_third_byte=%d\n",
           write_requests, file2_bytes[0] == (unsigned char)'C' ? 1 : 0);
    fflush(stdout);

    before = write_requests;
    int self = open(path2, O_RDWR);
    if (self < 0) {
        perror("daemon self-open");
        return 5;
    }
    if (fsync(self) != 0) perror("daemon self-open fsync");
    int flushed = wait_for_writes(before + 1, 5000, "post-self-fsync");
    printf("probe: per-file self-open post-fsync daemon_write_requests=%lu "
           "daemon_third_byte=%u observed_after_ms=%d third_store_wait_ms=%d\n",
           write_requests, file2_bytes[0], flushed, third);
    fflush(stdout);
    /* Open-unlinked case: the same inode has no path for a self-open. */
    if (unlink(path2) != 0) perror("unlink data2");
    mapping2[1] = 'D';
    __sync_synchronize();
    before = write_requests;
    int unlinked = wait_for_writes(before + 1, 300, "post-unlinked-store");
    int self2 = open(path2, O_RDWR);
    int self2_errno = errno;
    if (self2 >= 0) close(self2);
    printf("probe: open-unlinked daemon_saw_fourth_byte=%d self_open_errno=%d "
           "store_wait_ms=%d\n",
           file2_bytes[1] == (unsigned char)'D' ? 1 : 0, self2 >= 0 ? 0 : self2_errno, unlinked);
    fflush(stdout);
    before = write_requests;
    if (syncfs(dirfd) != 0) perror("syncfs unlinked");
    int unlinked_flush = wait_for_writes(before + 1, 5000, "post-unlinked-syncfs");
    printf("probe: open-unlinked after-syncfs daemon_fourth_byte=%u observed_after_ms=%d\n",
           file2_bytes[1], unlinked_flush);
    fflush(stdout);
    close(self);
    munmap((void *)mapping2, FILE_SIZE);
    close(fd2);

    munmap((void *)mapping, FILE_SIZE);
    close(fd);
    close(dirfd);
    msleep(200);
    if (umount2(mnt, MNT_DETACH) != 0) perror("umount2");
    return 0;
}
