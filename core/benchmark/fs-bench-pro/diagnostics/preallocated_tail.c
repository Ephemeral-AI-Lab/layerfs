/* Disposable APFS extent diagnosis, never a product speed arm. */
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2 && argc != 3) return 2;
    int fd = open(argv[1], O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd < 0) { perror("open"); return 1; }
    unsigned char data[4096], checked[4096];
    memset(data, 0xa5, sizeof(data));
    if (write(fd, data, sizeof(data)) != sizeof(data)) { perror("write"); return 1; }
    fstore_t allocation = {F_ALLOCATEALL, F_PEOFPOSMODE, 0, 16 * 1024 * 1024, 0};
    if (fcntl(fd, F_PREALLOCATE, &allocation)) { perror("preallocate"); return 1; }
    struct stat before, after;
    if (fstat(fd, &before)) return 1;
    if (argc == 3) {
        int sink = open(argv[2], O_CREAT | O_EXCL | O_RDWR, 0600);
        if (sink < 0 || fcntl(fd, F_TRANSFEREXTENTS, sink)) { perror("transfer extra extents"); return 1; }
        if (close(sink) || unlink(argv[2])) return 1;
    } else if (ftruncate(fd, before.st_size)) { perror("same-size truncate"); return 1; }
    if (fstat(fd, &after)) return 1;
    if (pread(fd, checked, sizeof(checked), 0) != sizeof(checked) || memcmp(data, checked, sizeof(data))) return 1;
    printf("{\"logical_before\":%lld,\"logical_after\":%lld,\"allocated_before\":%lld,\"allocated_after\":%lld,\"data_match\":true}\n",
           (long long)before.st_size, (long long)after.st_size,
           (long long)before.st_blocks * 512, (long long)after.st_blocks * 512);
    return close(fd) ? 1 : 0;
}
