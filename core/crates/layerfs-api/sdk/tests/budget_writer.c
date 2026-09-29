/* External POSIX/FUSE workload only. Build: zig cc -target aarch64-linux-musl
 * -static -O2 -o core/target/issue273-budget-writer core/crates/layerfs-api/sdk/tests/budget_writer.c
 * No LayerFS library, process hook, direct backing access or quota change.
 */
#include <fcntl.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
int main(int argc, char **argv) {
    if (argc != 4) return 2;
    unsigned long start = strtoul(argv[2], 0, 10), count = strtoul(argv[3], 0, 10);
    if (count > 4096 || start > 65536 || count > 65536 - start) return 3;
    if (strcmp(argv[1], "create") == 0) {
        for (unsigned long i = 0; i < count; i++) {
            char name[201];
            snprintf(name, sizeof(name), "f%06lu-", start + i);
            memset(name + 8, 'a' + (start + i) % 25, 180);
            name[188] = 0;
            int new_fd = open(name, O_CREAT | O_EXCL | O_WRONLY, 0644);
            if (new_fd < 0) { perror("create"); return 6; }
            if (write(new_fd, "x", 1) != 1 || close(new_fd) != 0) { perror("write"); return 7; }
        }
        return 0;
    }
    int fd = open(argv[1], O_WRONLY);
    if (fd < 0) { perror("open"); return 4; }
    for (unsigned long i = 0; i < count; i++) {
        unsigned long at = (start + i) * 2048 + 8;
        char byte = 'a' + (char)((start + i) % 23);
        if (pwrite(fd, &byte, 1, at) != 1) { perror("pwrite"); return 5; }
    }
    return close(fd) != 0;
}
