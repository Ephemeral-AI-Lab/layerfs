/* External read-only observation of SQLite's native RESERVED lock.
 * No product hook, SQL statement, lock acquisition, data read or write. */
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

int layerfs_reserved_lock_owner(const char *path) {
    int descriptor = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW);
    if (descriptor < 0) return -errno;
    struct stat metadata;
    if (fstat(descriptor, &metadata) < 0) {
        int error = errno;
        close(descriptor);
        return -error;
    }
    if (!S_ISREG(metadata.st_mode)) {
        close(descriptor);
        return -EINVAL;
    }
    struct flock lock = {0};
    lock.l_type = F_WRLCK;
    lock.l_whence = SEEK_SET;
    lock.l_start = ((off_t)1 << 30) + 1;
    lock.l_len = 1;
    if (fcntl(descriptor, F_GETLK, &lock) < 0) {
        int error = errno;
        close(descriptor);
        return -error;
    }
    close(descriptor);
    return lock.l_type == F_UNLCK ? 0 : lock.l_pid;
}
