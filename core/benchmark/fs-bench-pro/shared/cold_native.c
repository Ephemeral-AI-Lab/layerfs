/* Counted, bounded cold attestation. No file contents are read or faulted. */
#ifndef __APPLE__
#error "This cold helper requires the qualified macOS msync/mincore profile"
#endif
#include <errno.h>
#include <fcntl.h>
#include <fts.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

struct counts {
    uint64_t files, bytes, pages, resident_first, resident_after, invalidated;
    uint64_t opens, fstats, maps, mincores, unmaps, msyncs, entries, vector_peak;
    uint64_t first_fingerprint;
};
static struct counts c;
static size_t page;
static unsigned char *vector;
static size_t vector_capacity;
static uint64_t fingerprint;
static void mix(const void *value, size_t length) {
    const unsigned char *bytes = value;
    for (size_t i = 0; i < length; i++) { fingerprint ^= bytes[i]; fingerprint *= UINT64_C(1099511628211); }
}
static void fail(const char *what, const char *path) {
    fprintf(stderr, "cold helper: %s: %s: %s\n", what, path, strerror(errno));
    free(vector);
    exit(1);
}
static uint64_t now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) fail("clock", "");
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
}
static int order(const FTSENT **a, const FTSENT **b) {
    return strcmp((*a)->fts_name, (*b)->fts_name);
}
static int open_file(const FTSENT *entry, int flags, size_t *length, int inventory) {
    struct stat s;
    c.opens++;
    int fd = open(entry->fts_path, flags | O_NOFOLLOW | O_NONBLOCK);
    if (fd < 0) fail("open", entry->fts_path);
    c.fstats++;
    if (fstat(fd, &s)) fail("fstat", entry->fts_path);
    if (!S_ISREG(s.st_mode) || s.st_size < 0) {
        errno = EINVAL; fail("inventory identity changed", entry->fts_path);
    }
    *length = (size_t)s.st_size;
    if (inventory) {
        mix(entry->fts_path, strlen(entry->fts_path) + 1);
        mix(&s.st_dev, sizeof(s.st_dev)); mix(&s.st_ino, sizeof(s.st_ino)); mix(&s.st_size, sizeof(s.st_size));
    }
    return fd;
}
static void close_file(int fd, const char *path) {
    if (close(fd)) fail("close", path);
}
static void *map_file(int fd, size_t length, const char *path) {
    c.maps++;
    void *address = mmap(NULL, length, PROT_READ, MAP_SHARED, fd, 0);
    if (address == MAP_FAILED) fail("mmap", path);
    return address;
}
static uint64_t resident(void *address, size_t length, const char *path) {
    size_t pages = length / page + (length % page != 0);
    if (pages > vector_capacity) {
        unsigned char *next = realloc(vector, pages);
        if (!next) fail("mincore vector", path);
        vector = next; vector_capacity = pages; c.vector_peak = pages;
    }
    c.mincores++;
    if (mincore(address, length, (char *)vector)) fail("mincore", path);
    uint64_t n = 0;
    for (size_t i = 0; i < pages; i++) n += (vector[i] & 1) != 0;
    return n;
}
static void unmap_file(void *address, size_t length, const char *path) {
    c.unmaps++;
    if (munmap(address, length)) fail("munmap", path);
}
static uint64_t inspect(const FTSENT *entry, size_t *length) {
    int fd = open_file(entry, O_RDONLY, length, 1);
    if (!*length) { close_file(fd, entry->fts_path); return 0; }
    void *address = map_file(fd, *length, entry->fts_path);
    close_file(fd, entry->fts_path);
    uint64_t n = resident(address, *length, entry->fts_path);
    unmap_file(address, *length, entry->fts_path);
    return n;
}
static uint64_t precondition(const FTSENT *entry, size_t *length) {
    int fd = open_file(entry, O_RDONLY, length, 1);
    if (!*length) { close_file(fd, entry->fts_path); return 0; }
    void *address = map_file(fd, *length, entry->fts_path);
    close_file(fd, entry->fts_path);
    uint64_t n = resident(address, *length, entry->fts_path);
    if (n) {
        /* Preserve the existing writable invalidation capability check. The
           read-only shared mapping was never faulted and can be reused. */
        size_t confirmed;
        int writable = open_file(entry, O_RDWR, &confirmed, 0);
        close_file(writable, entry->fts_path);
        if (confirmed != *length) { errno = EINVAL; fail("invalidation size changed", entry->fts_path); }
        c.msyncs++;
        if (msync(address, *length, MS_INVALIDATE)) fail("msync invalidate", entry->fts_path);
        c.invalidated++;
        (void)resident(address, *length, entry->fts_path);
    }
    unmap_file(address, *length, entry->fts_path);
    return n;
}
static void walk(char **paths, int final) {
    const char *root = paths[0];
    FTS *tree = fts_open(paths, FTS_PHYSICAL | FTS_NOCHDIR | FTS_NOSTAT, order);
    if (!tree) fail("fts_open", root);
    FTSENT *entry;
    uint64_t files = 0, bytes = 0, pages = 0;
    fingerprint = UINT64_C(14695981039346656037);
    errno = 0;
    while ((entry = fts_read(tree))) {
        c.entries++;
        if (entry->fts_info == FTS_D || entry->fts_info == FTS_DP) continue;
        if (entry->fts_info != FTS_F && entry->fts_info != FTS_NSOK) { errno = EINVAL; fail("nonregular entry", entry->fts_path); }
        size_t length;
        uint64_t n = final ? inspect(entry, &length) : precondition(entry, &length);
        files++; bytes += length; pages += length / page + (length % page != 0);
        if (final) c.resident_after += n;
        else c.resident_first += n;
        errno = 0;
    }
    if (errno) fail("fts_read", root);
    if (fts_close(tree)) fail("fts_close", root);
    if (!files) { errno = EINVAL; fail("empty source inventory", root); }
    if (final) {
        if (files != c.files || bytes != c.bytes || pages != c.pages || fingerprint != c.first_fingerprint) {
            errno = EINVAL; fail("whole-tree inventory changed", root);
        }
    } else { c.files = files; c.bytes = bytes; c.pages = pages; c.first_fingerprint = fingerprint; }
}
static void files(char **paths, int count, int final) {
    uint64_t bytes = 0, pages = 0;
    fingerprint = UINT64_C(14695981039346656037);
    for (int i = 0; i < count; i++) {
        struct stat s;
        if (lstat(paths[i], &s) || !S_ISREG(s.st_mode)) {
            errno = EINVAL; fail("ordinary file required", paths[i]);
        }
        for (int j = 0; j < i; j++) {
            struct stat previous;
            if (lstat(paths[j], &previous)) fail("inventory", paths[j]);
            if (s.st_dev == previous.st_dev && s.st_ino == previous.st_ino) {
                errno = EINVAL; fail("duplicate file identity", paths[i]);
            }
        }
        FTSENT entry = {0}; entry.fts_path = paths[i];
        size_t length;
        uint64_t n = final ? inspect(&entry, &length) : precondition(&entry, &length);
        bytes += length; pages += length / page + (length % page != 0);
        if (final) c.resident_after += n; else c.resident_first += n;
        c.entries++;
    }
    if (final) {
        if (c.files != (uint64_t)count || c.bytes != bytes || c.pages != pages || fingerprint != c.first_fingerprint) {
            errno = EINVAL; fail("file inventory changed", paths[0]);
        }
    } else { c.files = (uint64_t)count; c.bytes = bytes; c.pages = pages; c.first_fingerprint = fingerprint; }
}
int main(int argc, char **argv) {
    int file_mode = argc >= 3 && !strcmp(argv[1], "--files");
    int paths_mode = argc >= 3 && !strcmp(argv[1], "--paths");
    if (!file_mode && !paths_mode && argc != 2) {
        fprintf(stderr, "usage: phase7-cold SOURCE | --files FILE... | --paths PATH...\n"); return 2;
    }
    if (!file_mode && !paths_mode) {
        struct stat s;
        if (lstat(argv[1], &s) || !S_ISDIR(s.st_mode)) { errno = EINVAL; fail("ordinary directory required", argv[1]); }
    }
    long value = sysconf(_SC_PAGESIZE);
    if (value <= 0) fail("page size", argv[1]);
    page = (size_t)value;
    uint64_t begin = now();
    if (file_mode) files(argv + 2, argc - 2, 0); else walk(argv + (paths_mode ? 2 : 1), 0);
    uint64_t middle = now();
    if (file_mode) files(argv + 2, argc - 2, 1); else walk(argv + (paths_mode ? 2 : 1), 1);
    uint64_t end = now();
    printf("{\"files\":%"PRIu64",\"length_bytes\":%"PRIu64",\"total_pages\":%"PRIu64
           ",\"resident_first\":%"PRIu64",\"resident_after\":%"PRIu64",\"invalidated_files\":%"PRIu64
           ",\"page_size_bytes\":%zu,\"first_pass_ns\":%"PRIu64",\"attestation_ns\":%"PRIu64
           ",\"opens\":%"PRIu64",\"fstats\":%"PRIu64",\"mmap_calls\":%"PRIu64
           ",\"mincore_calls\":%"PRIu64",\"munmap_calls\":%"PRIu64",\"msync_calls\":%"PRIu64
           ",\"inventory_entries\":%"PRIu64",\"vector_peak_bytes\":%"PRIu64"}\n",
           c.files,c.bytes,c.pages,c.resident_first,c.resident_after,c.invalidated,page,middle-begin,end-middle,
           c.opens,c.fstats,c.maps,c.mincores,c.unmaps,c.msyncs,c.entries,c.vector_peak);
    free(vector); return 0;
}
