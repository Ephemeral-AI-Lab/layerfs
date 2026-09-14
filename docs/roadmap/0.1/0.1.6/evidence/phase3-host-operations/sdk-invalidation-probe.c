/* Changed SDK-coherence hypothesis, not a Commit/V1 snapshot proof.
 * Reuse the predecessor's cached FUSE server. Intercept its backing copy only
 * to model an immutable post-SDK byte protected during queued folio writeback.
 * No NOTIFY_STORE is used; invalidation must preserve the mapped reader and
 * unrelated dirty bytes. An injected laundering failure must be observable.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <stdatomic.h>
#include <string.h>
#include <unistd.h>
#include <sys/types.h>

static void *sdk_copy(void *dst, const void *src, size_t len);
static ssize_t sdk_read(int fd, void *bytes, size_t len);
static ssize_t sdk_write(int fd, const void *bytes, size_t len);
#define memcpy sdk_copy
#define read sdk_read
#define write sdk_write
#define main original_probe_main
#include "../v1-probe/probe.c"
#undef main
#undef write
#undef read
#undef memcpy

static pthread_mutex_t backing_lock = PTHREAD_MUTEX_INITIALIZER;
static atomic_int protecting, fail_write, protected_byte, failed_writes;
static atomic_uint_fast64_t write_unique;

static int backing_address(const void *ptr) {
    uintptr_t at = (uintptr_t)ptr;
    return at >= (uintptr_t)file_bytes && at < (uintptr_t)(file_bytes + FILE_SIZE);
}
static void *sdk_copy(void *dst, const void *src, size_t len) {
    int destination = backing_address(dst);
    int backing = destination || backing_address(src);
    if (backing) pthread_mutex_lock(&backing_lock);
    memcpy(dst, src, len);
    if (destination && atomic_load(&protecting)) {
        size_t start = (unsigned char *)dst - file_bytes;
        if (start <= 16 && 16 < start + len) file_bytes[16] = (unsigned char)atomic_load(&protected_byte);
    }
    if (backing) pthread_mutex_unlock(&backing_lock);
    return dst;
}
static ssize_t sdk_read(int fd, void *bytes, size_t len) {
    ssize_t count = read(fd, bytes, len);
    if (fd == fuse_fd && count >= (ssize_t)sizeof(struct fuse_in_header)) {
        const struct fuse_in_header *header = bytes;
        if (header->opcode == FUSE_WRITE) atomic_store(&write_unique, header->unique);
    }
    return count;
}
static ssize_t sdk_write(int fd, const void *bytes, size_t len) {
    if (fd == fuse_fd && len >= sizeof(struct fuse_out_header)) {
        struct fuse_out_header header;
        memcpy(&header, bytes, sizeof(header));
        if (header.unique && header.unique == atomic_load(&write_unique) && atomic_load(&fail_write)) {
            header.error = -EIO;
            header.len = sizeof(header);
            atomic_fetch_add(&failed_writes, 1);
            ssize_t sent = write(fd, &header, sizeof(header));
            return sent == (ssize_t)sizeof(header) ? (ssize_t)len : -1;
        }
    }
    return write(fd, bytes, len);
}
static int invalidate(void) {
    struct { struct fuse_out_header header; struct fuse_notify_inval_inode_out inode; } packet;
    memset(&packet, 0, sizeof(packet));
    packet.header.len = sizeof(packet);
    packet.header.error = FUSE_NOTIFY_INVAL_INODE;
    packet.inode.ino = NODE_FILE;
    packet.inode.off = 0;
    packet.inode.len = 0;
    return write(fuse_fd, &packet, sizeof(packet)) == (ssize_t)sizeof(packet) ? 0 : -1;
}
static void install_sdk(unsigned char byte) {
    pthread_mutex_lock(&backing_lock);
    file_bytes[16] = byte;
    atomic_store(&protected_byte, byte);
    atomic_store(&protecting, 1);
    pthread_mutex_unlock(&backing_lock);
}
int main(void) {
    const char *mnt = "/tmp/layerfs-sdk-invalidation-probe";
    assert(mkdir(mnt, 0700) == 0 || errno == EEXIST);
    memset(file_bytes, 'A', sizeof(file_bytes));
    fuse_fd = open("/dev/fuse", O_RDWR);
    assert(fuse_fd >= 0);
    char options[256];
    snprintf(options, sizeof(options), "fd=%d,rootmode=40000,user_id=0,group_id=0,max_read=1048576", fuse_fd);
    assert(mount("sdk-probe", mnt, "fuse", MS_NOSUID | MS_NODEV, options) == 0);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, server, NULL) == 0);
    int directory = open(mnt, O_RDONLY | O_DIRECTORY);
    char path[256]; snprintf(path, sizeof(path), "%s/data", mnt);
    int file = open(path, O_RDWR);
    assert(directory >= 0 && file >= 0);
    volatile unsigned char *mapping = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, file, 0);
    assert(mapping != MAP_FAILED && mapping[16] == 'A');

    /* Explicit SDK BEGIN flushes prior dirty data, but the writable mapping
     * remains open and can refault/dirty a page before the host SDK install. */
    mapping[128] = 'm';
    assert(invalidate() == 0 && syncfs(directory) == 0);
    mapping[128] = 'u';
    assert(mapping[16] == 'A');
    install_sdk('S');
    unsigned long before = write_requests;
    int invalidation = invalidate();
    int flushed = syncfs(directory);
    printf("SDK_NO_STORE: inval=%d syncfs=%d mapped_sdk=%u mapped_other=%u backing_sdk=%u backing_other=%u laundering_writes=%lu\n",
           invalidation, flushed, mapping[16], mapping[128], file_bytes[16], file_bytes[128], write_requests-before);
    fflush(stdout);
    assert(invalidation == 0 && flushed == 0 && mapping[16] == 'S' && mapping[128] == 'u');
    assert(file_bytes[16] == 'S' && file_bytes[128] == 'u');

    /* FUSE invalidation ignores the page-laundering error. The pre-existing
     * directory fd must let syncfs report it before an SDK success is returned. */
    mapping[128] = 'z';
    install_sdk('T');
    atomic_store(&fail_write, 1);
    errno = 0;
    invalidation = invalidate();
    int invalidation_errno = errno;
    errno = 0;
    flushed = syncfs(directory);
    int flush_errno = errno;
    printf("SDK_LAUNDER_FAILURE: inval=%d inval_errno=%d syncfs=%d syncfs_errno=%d rejected_writes=%d mapped_sdk=%u backing_sdk=%u\n",
           invalidation, invalidation_errno, flushed, flush_errno, atomic_load(&failed_writes), mapping[16], file_bytes[16]);
    fflush(stdout);
    assert(atomic_load(&failed_writes) > 0 && invalidation == 0 && flushed < 0 && flush_errno == EIO);
    atomic_store(&fail_write, 0);
    assert(invalidate() == 0);
    (void)syncfs(directory);
    printf("SDK_RETRY: mapped_sdk=%u mapped_other=%u backing_sdk=%u backing_other=%u\n", mapping[16], mapping[128], file_bytes[16], file_bytes[128]);
    fflush(stdout);
    assert(mapping[16] == 'T' && mapping[128] == 'z');
    atomic_store(&protecting, 0);
    munmap((void *)mapping, FILE_SIZE); close(file); close(directory);
    assert(umount2(mnt, MNT_DETACH) == 0);
    puts("SDK invalidation/protected-writeback hypotheses PASS; Commit V1 remains OPEN");
    return 0;
}
