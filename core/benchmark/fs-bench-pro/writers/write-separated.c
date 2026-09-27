/* One ordinary mounted file descriptor and N one-byte positional writes. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

static uint64_t nanos(void) {
    struct timespec value;
    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) {
        perror("clock_gettime");
        exit(1);
    }
    return (uint64_t)value.tv_sec * 1000000000u + (uint64_t)value.tv_nsec;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fputs("usage: write-separated FILE COUNT\n", stderr);
        return 2;
    }
    char *end;
    errno = 0;
    unsigned long count = strtoul(argv[2], &end, 10);
    if (errno || *end || count == 0 || count > 4097) {
        fputs("invalid write count\n", stderr);
        return 2;
    }
    int fd = open(argv[1], O_WRONLY | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) {
        perror("open");
        return 1;
    }
    uint64_t start = nanos();
    const unsigned long checkpoints[] = {count / 4, count / 2, count * 3 / 4, count};
    for (unsigned long i = 0; i < count; ++i) {
        if (pwrite(fd, "X", 1, (off_t)(2 * i)) != 1) {
            perror("pwrite");
            close(fd);
            return 1;
        }
        for (unsigned j = 0; j < 4; ++j) {
            if (i + 1 == checkpoints[j]) {
                printf("PROGRESS\t%lu\t%llu\n", i + 1,
                       (unsigned long long)(nanos() - start));
                fflush(stdout);
            }
        }
    }
    if (close(fd) != 0) {
        perror("close");
        return 1;
    }
    return 0;
}
