/* Diagnostic only: memory.peak reset and read through the same open FD. */
#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

static unsigned long long value(int fd) {
    char text[64] = {0};
    assert(lseek(fd, 0, SEEK_SET) == 0);
    assert(read(fd, text, sizeof(text) - 1) > 0);
    return strtoull(text, NULL, 10);
}
static unsigned long long current(void) {
    int fd = open("/sys/fs/cgroup/memory.current", O_RDONLY);
    assert(fd >= 0);
    unsigned long long result = value(fd);
    assert(close(fd) == 0);
    return result;
}
static void *charge(size_t size) {
    volatile unsigned char *bytes = mmap(NULL, size, PROT_READ | PROT_WRITE,
                                         MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert((void *)bytes != MAP_FAILED);
    for (size_t at = 0; at < size; at += 4096) bytes[at] = 1;
    return (void *)bytes;
}
int main(void) {
    int fd = open("/sys/fs/cgroup/memory.peak", O_RDWR);
    assert(fd >= 0);
    void *setup = charge(32 * 1024 * 1024);
    unsigned long long setup_peak = value(fd);
    assert(setup_peak >= 32 * 1024 * 1024);
    assert(munmap(setup, 32 * 1024 * 1024) == 0);
    unsigned long long before = current();
    assert(lseek(fd, 0, SEEK_SET) == 0 && write(fd, "0", 1) == 1);
    unsigned long long reset = value(fd);
    assert(reset < setup_peak / 2);
    void *phase = charge(4 * 1024 * 1024);
    unsigned long long during = current();
    assert(munmap(phase, 4 * 1024 * 1024) == 0);
    unsigned long long peak = value(fd);
    int other = open("/sys/fs/cgroup/memory.peak", O_RDONLY);
    assert(other >= 0);
    unsigned long long lifetime = value(other);
    assert(peak >= during && peak > reset && peak < setup_peak / 2);
    assert(lifetime >= setup_peak);
    printf("{\"status\":\"PASS\",\"kind\":\"host-capability-diagnostic\","
           "\"same_fd\":true,\"setup_peak_bytes\":%llu,\"before_bytes\":%llu,"
           "\"reset_peak_bytes\":%llu,\"phase_current_bytes\":%llu,"
           "\"phase_peak_bytes\":%llu,\"other_fd_lifetime_bytes\":%llu}\n",
           setup_peak, before, reset, during, peak, lifetime);
    assert(close(other) == 0 && close(fd) == 0);
    return 0;
}
