/* v1-probe-unmeasured / enumdirty VERSION 1 (preserved).
 *
 * Raw output: run-enumdirty.log (+ .json). Two weak spots fixed in enumdirty.c:
 *   (1) ED-3 could not distinguish "PAGE_IS_WRITTEN means dirty" from
 *       "PAGE_IS_WRITTEN means present and not uffd-wp'd", because every page
 *       that was present had also been written;
 *   (2) ED-4 read the soft-dirty bit for accesses made *before* the
 *       clear_refs=4, which cannot show that later writes are untracked.
 *
 * Original header follows.
 *
 * == "can the daemon learn that a mapping exists and which
 * pages are dirty?" -- the cheap extra asked for by the task.
 *
 * Candidates the kernel documents:
 *   - /proc/<pid>/maps: mapping existence (needs ptrace-read permission on the
 *     target process).
 *   - /proc/<pid>/pagemap: per-page flags, including bit 55 soft-dirty
 *     (needs CONFIG_MEM_SOFT_DIRTY) and the PAGEMAP_SCAN ioctl (6.7+) with the
 *     PAGE_IS_WRITTEN / PAGE_IS_SOFT_DIRTY categories.
 *   - clear_refs "4" (clear soft-dirty): needs CONFIG_MEM_SOFT_DIRTY.
 *
 * This probe measures what is actually available on the deployed kernel for
 * *another* process's writable shared mapping:
 *   ED-1  is /proc/<pid>/maps readable (mapping existence)?
 *   ED-2  what does clear_refs "4" do?
 *   ED-3  does PAGEMAP_SCAN's PAGE_IS_WRITTEN category distinguish written from
 *         unwritten pages when the range was never registered with userfaultfd?
 *   ED-4  do the raw pagemap soft-dirty bits distinguish them?
 *
 * Child: mmaps a 32-page tmpfs file MAP_SHARED|PROT_WRITE and writes pages 1
 * and 5 only. Parent: scans the child's mapping.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define PAGE_SZ 4096u
#define NPAGES 32u

/* uapi 6.12 (container headers are 6.1) */
struct un_page_region {
    uint64_t start, end, categories;
};
struct un_pm_scan_arg {
    uint64_t size, flags, start, end, walk_end, vec, vec_len, max_pages;
    uint64_t category_inverted, category_mask, category_anyof_mask, return_mask;
};
#define UN_PAGE_IS_WPALLOWED (1 << 0)
#define UN_PAGE_IS_WRITTEN   (1 << 1)
#define UN_PAGE_IS_FILE      (1 << 2)
#define UN_PAGE_IS_PRESENT   (1 << 3)
#define UN_PAGE_IS_SWAPPED   (1 << 4)
#define UN_PAGE_IS_SOFT_DIRTY (1 << 7)
#define UN_PAGEMAP_SCAN _IOWR('f', 16, struct un_pm_scan_arg)

static void cat_flags(uint64_t c, char *out, size_t n) {
    out[0] = 0;
    if (c & UN_PAGE_IS_WPALLOWED) strncat(out, "WPALLOWED|", n - strlen(out) - 1);
    if (c & UN_PAGE_IS_WRITTEN) strncat(out, "WRITTEN|", n - strlen(out) - 1);
    if (c & UN_PAGE_IS_FILE) strncat(out, "FILE|", n - strlen(out) - 1);
    if (c & UN_PAGE_IS_PRESENT) strncat(out, "PRESENT|", n - strlen(out) - 1);
    if (c & UN_PAGE_IS_SWAPPED) strncat(out, "SWAPPED|", n - strlen(out) - 1);
    if (c & UN_PAGE_IS_SOFT_DIRTY) strncat(out, "SOFT_DIRTY|", n - strlen(out) - 1);
    if (!out[0]) strcpy(out, "none");
}

int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("probe: page_size=%ld pages=%u\n", sysconf(_SC_PAGESIZE), NPAGES);

    int bfd = open("/dev/shm/v1un-edirty", O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (bfd < 0) { perror("open backing"); return 2; }
    if (ftruncate(bfd, NPAGES * PAGE_SZ) != 0) { perror("ftruncate"); return 2; }

    pid_t child = fork();
    if (child == 0) {
        int fd = open("/dev/shm/v1un-edirty", O_RDWR);
        volatile unsigned char *m =
            mmap(NULL, NPAGES * PAGE_SZ, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if (m == MAP_FAILED) { perror("child mmap"); _exit(3); }
        /* write ONLY pages 1 and 5 */
        *(volatile unsigned long long *)(m + 1 * PAGE_SZ) = 0x1111111111111111ULL;
        *(volatile unsigned long long *)(m + 5 * PAGE_SZ) = 0x5555555555555555ULL;
        __sync_synchronize();
        /* stay alive; report that the mapping is ready */
        printf("child: mapped and wrote pages 1 and 5 only\n");
        fflush(stdout);
        for (;;) pause();
    }
    if (child < 0) { perror("fork"); return 2; }
    sleep(1);

    char p[128];
    unsigned long long vstart = 0, vend = 0;

    /* ED-1: mapping existence via /proc/<pid>/maps */
    {
        snprintf(p, sizeof(p), "/proc/%d/maps", (int)child);
        FILE *f = fopen(p, "r");
        printf("ED-1 /proc/<pid>/maps open=%s\n", f ? "ok" : "FAILED");
        if (f) {
            char line[512];
            while (fgets(line, sizeof(line), f)) {
                if (strstr(line, "v1un-edirty")) {
                    printf("ED-1 maps_line=%s", line);
                    sscanf(line, "%llx-%llx", &vstart, &vend);
                }
            }
            fclose(f);
        }
        printf("RESULT ED1 mapping_existence_learnable=%d range=0x%llx-0x%llx\n", vstart != 0,
               vstart, vend);
    }

    /* ED-2: clear_refs "4" (clear soft-dirty) */
    {
        snprintf(p, sizeof(p), "/proc/%d/clear_refs", (int)child);
        int fd = open(p, O_WRONLY);
        errno = 0;
        ssize_t w = -1;
        if (fd >= 0) w = write(fd, "4", 1);
        int e = errno;
        if (fd >= 0) close(fd);
        printf("ED-2 clear_refs=4 write rc=%zd errno=%d (%s)\n", w, e, strerror(e));
        printf("RESULT ED2 soft_dirty_clear_available=%d errno=%d\n", w == 1, e);
    }

    /* ED-3: PAGEMAP_SCAN for PAGE_IS_WRITTEN / PAGE_IS_SOFT_DIRTY */
    {
        snprintf(p, sizeof(p), "/proc/%d/pagemap", (int)child);
        int fd = open(p, O_RDONLY);
        printf("ED-3 pagemap open=%s errno=%d\n", fd >= 0 ? "ok" : "FAILED", errno);
        if (fd >= 0 && vstart) {
            struct un_page_region vec[NPAGES];
            memset(vec, 0, sizeof(vec));
            struct un_pm_scan_arg a;
            memset(&a, 0, sizeof(a));
            a.size = sizeof(a);
            a.flags = 0;
            a.start = vstart;
            a.end = vend;
            a.vec = (uint64_t)(uintptr_t)&vec[0];
            a.vec_len = NPAGES;
            a.max_pages = 0;
            a.category_mask = UN_PAGE_IS_WRITTEN;
            a.return_mask = UN_PAGE_IS_WRITTEN | UN_PAGE_IS_PRESENT | UN_PAGE_IS_FILE |
                            UN_PAGE_IS_SOFT_DIRTY;
            errno = 0;
            int rc = ioctl(fd, UN_PAGEMAP_SCAN, &a);
            int e = errno;
            printf("ED-3 PAGEMAP_SCAN(PAGE_IS_WRITTEN) rc=%d errno=%d (%s) walk_end=0x%llx\n", rc, e,
                   strerror(e), (unsigned long long)a.walk_end);
            unsigned reported_written = 0, reported_pages = 0;
            for (unsigned i = 0; i < NPAGES; i++) {
                if (!vec[i].start) continue;
                char fl[128];
                cat_flags(vec[i].categories, fl, sizeof(fl));
                printf("ED-3 region[%u] 0x%llx-0x%llx categories=%s\n", i,
                       (unsigned long long)vec[i].start, (unsigned long long)vec[i].end, fl);
                reported_pages += (unsigned)((vec[i].end - vec[i].start) / PAGE_SZ);
                if (vec[i].categories & UN_PAGE_IS_WRITTEN)
                    reported_written += (unsigned)((vec[i].end - vec[i].start) / PAGE_SZ);
            }
            printf("RESULT ED3 scan_rc=%d pages_reported=%u pages_reported_WRITTEN=%u "
                   "pages_actually_written_by_the_mapping=2\n",
                   rc, reported_pages, reported_written);
            close(fd);
        }
    }

    /* ED-4: raw pagemap soft-dirty bits for each page of the mapping */
    {
        snprintf(p, sizeof(p), "/proc/%d/pagemap", (int)child);
        int fd = open(p, O_RDONLY);
        if (fd >= 0 && vstart) {
            unsigned long long vpn = vstart / PAGE_SZ;
            unsigned present = 0, dirty_bits = 0;
            for (unsigned i = 0; i < NPAGES; i++) {
                uint64_t entry = 0;
                off_t off = (off_t)((vpn + i) * 8);
                ssize_t r = pread(fd, &entry, 8, off);
                if (r != 8) continue;
                int preset = (entry >> 63) & 1;
                int soft_dirty = (entry >> 55) & 1;
                int file_shared = (entry >> 61) & 1;
                int uffd_wp = (entry >> 57) & 1;
                if (preset) present++;
                if (soft_dirty) dirty_bits++;
                if (i == 0 || i == 1 || i == 5)
                    printf("ED-4 page %u present=%d soft_dirty_bit55=%d file_bit61=%d "
                           "uffd_wp_bit57=%d\n",
                           i, preset, soft_dirty, file_shared, uffd_wp);
            }
            printf("RESULT ED4 pages_present=%u pages_with_soft_dirty_bit=0x%x\n", present,
                   dirty_bits);
            close(fd);
        }
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    close(bfd);
    printf("probe: done\n");
    return 0;
}
