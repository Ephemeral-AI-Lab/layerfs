/* External proof fixture: translate only a real failed pwrite's ENOSPC to EIO.
 * The syscall runs once, with unchanged arguments. No product injection, VFS
 * replacement, SQLite patch, success fabrication or failed write replay.
 * This exercises SQLite's IOERR rollback path on an actually full device.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <sys/types.h>
#include <unistd.h>

ssize_t pwrite(int fd, const void *bytes, size_t count, off_t at) {
    ssize_t (*original)(int, const void *, size_t, off_t) = dlsym(RTLD_NEXT, "pwrite");
    ssize_t result = original(fd, bytes, count, at);
    if (result < 0 && errno == ENOSPC) errno = EIO;
    return result;
}

ssize_t pwrite64(int fd, const void *bytes, size_t count, off64_t at) {
    ssize_t (*original)(int, const void *, size_t, off64_t) = dlsym(RTLD_NEXT, "pwrite64");
    ssize_t result = original(fd, bytes, count, at);
    if (result < 0 && errno == ENOSPC) errno = EIO;
    return result;
}
