/* One mounted fd; separated, append, dispersed, or repeated one-byte writes. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
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
    if (argc != 3 && argc != 4) {
        fputs("usage: write-separated [append|dispersed|repeated] FILE COUNT\n", stderr);
        return 2;
    }
    const char *mode = argc == 4 ? argv[1] : "separated";
    if (strcmp(mode, "separated") && strcmp(mode, "append") &&
        strcmp(mode, "dispersed") && strcmp(mode, "repeated")) {
        fputs("invalid write mode\n", stderr);
        return 2;
    }
    char *end;
    errno = 0;
    unsigned long count = strtoul(argv[argc - 1], &end, 10);
    if (errno || *end || count == 0 || count > 4097) {
        fputs("invalid write count\n", stderr);
        return 2;
    }
    int kind = mode[0];
    int append = kind == 'a';
    int fd = open(argv[argc - 2], O_WRONLY | O_CLOEXEC | O_NOFOLLOW |
                                      (append ? O_APPEND : 0));
    if (fd < 0) {
        perror("open");
        return 1;
    }
    uint64_t start = nanos();
    const unsigned long checkpoints[] = {count / 4, count / 2, count * 3 / 4, count};
    for (unsigned long i = 0; i < count; ++i) {
        unsigned char value = kind == 's' ? 'X' : 'B' + i % 24;
        off_t offset = kind == 'r' ? 5242880 :
            kind == 'd' ?
            (off_t)((104729ULL + (uint64_t)i * 2654435761ULL) % 10485760ULL) :
            (off_t)(2 * i);
        if ((append ? write(fd, &value, 1) : pwrite(fd, &value, 1, offset)) != 1) {
            perror(append ? "write" : "pwrite");
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
