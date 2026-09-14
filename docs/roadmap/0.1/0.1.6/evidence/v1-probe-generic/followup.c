/* Follow-up probe. Three measurements:
 *
 * T8  control: does merely parking the daemon (no flush) throttle the mapped
 *     writer? Establishes the temporal resolution of the parked experiments.
 * T7  with a queued writeback WRITE held for 15 ms, does the payload the daemon
 *     finally receives correspond to the flush request instant, the unpark
 *     instant, or the read instant? (aliasing vs owned copy timing)
 * T9  whole-range coherence: 4 MiB mapped file (1024 pages = 32 writeback
 *     requests at the negotiated max_pages), daemon-initiated fsync; is the
 *     union of every delivered page one single-instant image?
 *
 * Usage: ./followup [writeback]
 */
#define main probe_main_disabled
#include "probe.c"
#undef main

static pthread_t fsync_thread;
static int fsync_rc;
static int fsync_fd;

static void *fsync_main(void *unused) {
    (void)unused;
    fsync_rc = fsync(fsync_fd);
    return NULL;
}

static void print_pages(const char *tag, unsigned n) {
    printf("%s page_values:", tag);
    for (unsigned p = 0; p < n; p++) printf(" %llu", drain_val[p]);
    printf("\n");
}

static void drain_reset(unsigned npages) {
    g_drain_npages = npages;
    memset(drain_seen, 0, sizeof(drain_seen));
    memset(drain_val, 0, sizeof(drain_val));
    drain_writes = drain_write_cache = drain_pages = 0;
}

static void drain_stats(const char *tag, unsigned npages, unsigned long rr0, unsigned long rr1) {
    unsigned long long v[DRAIN_MAX];
    for (unsigned p = 0; p < npages; p++) v[p] = drain_val[p];
    struct inv iv = check_ordered(v, drain_seen, npages);
    unsigned long long mn = ~0ULL, mx = 0;
    for (unsigned p = 0; p < npages; p++) {
        if (!drain_seen[p]) continue;
        if (v[p] < mn) mn = v[p];
        if (v[p] > mx) mx = v[p];
    }
    printf("%s daemon_writes=%lu write_cache_flagged=%lu distinct_pages=%lu/%u "
           "payload_min=%llu payload_max=%llu payload_spread_rounds=%llu compared_pairs=%lu "
           "violating_pairs=%lu writer_rounds_during=%lu\n",
           tag, drain_writes, drain_write_cache, drain_pages, npages,
           mn == ~0ULL ? 0 : mn, mx, mx - (mn == ~0ULL ? 0 : mn), iv.pairs, iv.viol, rr1 - rr0);
}

int main(int argc, char **argv) {
    struct rlimit rl = {0, 0};
    setrlimit(RLIMIT_CORE, &rl);
    signal(SIGALRM, on_alarm);
    alarm(180);
    if (argc > 1) probe_argv1 = argv[1];
    writeback_mode = (argc > 1 && strcmp(argv[1], "writeback") == 0);

    CHK(mkdir(MNT, 0755) == 0 || errno == EEXIST);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    CHK(fuse_fd >= 0);
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", fuse_fd, S_IFDIR,
             getuid(), getgid());
    CHK(mount("v1generic", MNT, "fuse", MS_NOSUID | MS_NODEV, opts) == 0);
    mounted = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t sthread;
    CHK(pthread_create(&sthread, NULL, server, NULL) == 0);

    char path[256], bigpath[256];
    snprintf(path, sizeof(path), "%s/data", MNT);
    snprintf(bigpath, sizeof(bigpath), "%s/big", MNT);
    int fd = open(path, O_RDWR);
    CHK(fd >= 0);
    map0 = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    CHK(map0 != MAP_FAILED);
    fsync_fd = open(path, O_RDWR);
    CHK(fsync_fd >= 0);
    printf("probe: negotiated minor=%d flags=0x%x writeback_cache=%u\n", negotiated_minor,
           negotiated_flags, (negotiated_flags & FUSE_WRITEBACK_CACHE) ? 1u : 0u);

    /* ---- T8 + T7 on the 32-page file ---- */
    g_writer_npages = NPAGES;
    writer_start(1);
    msleep(200);

    /* T8: park the daemon 15 ms with no flush pending. */
    for (unsigned t = 0; t < 3; t++) {
        unsigned long rd0 = read_requests, wr0 = write_requests;
        server_park = 1;
        for (int i = 0; i < 5000 && !server_parked; i++) msleep(1);
        CHK(server_parked);
        unsigned long rr0 = writer_round;
        msleep(15);
        unsigned long rr1 = writer_round;
        printf("T8 trial=%u park_ms=15 writer_rounds_during_plain_park=%lu rounds_per_us=%.3f "
               "daemon_reads_during=%lu daemon_writes_during=%lu\n",
               t, rr1 - rr0, (rr1 - rr0) / 15.0, read_requests - rd0, write_requests - wr0);
        server_park = 0;
        msleep(5);
    }

    for (unsigned t = 0; t < 8; t++) {
        drain_reset(NPAGES);
        server_park = 1;
        for (int i = 0; i < 5000 && !server_parked; i++) msleep(1);
        CHK(server_parked);
        unsigned long rr_park = writer_round;
        CHK(pthread_create(&fsync_thread, NULL, fsync_main, NULL) == 0);
        msleep(15);
        unsigned long rr_hold = writer_round;
        server_park = 0;
        CHK(pthread_join(fsync_thread, NULL) == 0);
        unsigned long rr_after = writer_round;
        drain_stats("T7", NPAGES, rr_park, rr_after);
        printf("T7 trial=%u fsync_rc=%d writer_round_park=%lu hold=%lu read=%lu "
               "payload_max_minus_park=%lld payload_max_minus_hold=%lld "
               "payload_max_minus_read=%lld\n",
               t, fsync_rc, rr_park, rr_hold, rr_after,
               (long long)drain_val[0] - (long long)rr_park,
               (long long)drain_val[0] - (long long)rr_hold,
               (long long)drain_val[0] - (long long)rr_after);
        if (t == 0) print_pages("T7 trial=0", NPAGES);
    }
    writer_stop_and_join();

    /* ---- T9: whole-range coherence over 4 MiB ---- */
    int bigfd = open(bigpath, O_RDWR);
    CHK(bigfd >= 0);
    volatile unsigned char *bigmap =
        mmap(NULL, FILE_BIG_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, bigfd, 0);
    CHK(bigmap != MAP_FAILED);
    map0 = bigmap;
    g_writer_npages = BIG_PAGES;
    unsigned long pr0 = writer_round;
    writer_start(1);
    msleep(600);
    unsigned long pr1 = writer_round;
    printf("T9 prefault_big pages=%u writer_rounds=%lu daemon_reads=%lu daemon_writes=%lu\n",
           BIG_PAGES, pr1 - pr0, read_requests, write_requests);
    for (unsigned t = 0; t < 3; t++) {
        drain_reset(BIG_PAGES);
        unsigned long wr0 = write_requests;
        unsigned long rr0 = writer_round;
        int rc = fsync(bigfd);
        unsigned long rr1 = writer_round;
        printf("T9 trial=%u fsync_rc=%d daemon_write_requests=%lu write_cache_flagged=%lu\n", t, rc,
               write_requests - wr0, drain_write_cache);
        drain_stats("T9", BIG_PAGES, rr0, rr1);
        printf("T9 trial=%u sampled_pages p0=%llu p256=%llu p512=%llu p768=%llu p1023=%llu "
               "current_mapping_p0=%llu\n",
               t, drain_val[0], drain_val[256], drain_val[512], drain_val[768], drain_val[1023],
               *(volatile unsigned long long *)bigmap);
    }
    writer_stop_and_join();

    printf("probe: done. totals reads=%lu writes=%lu write_cache=%lu\n", read_requests,
           write_requests, write_cache_requests);
    munmap((void *)bigmap, FILE_BIG_SIZE);
    close(bigfd);
    /* Unmap every writable mapping and close the fds while the daemon is alive;
     * exiting with dirty mappings of a live FUSE mount wedges the container. */
    if (map1 && map1 != MAP_FAILED) munmap((void *)map1, FILE_SIZE);
    if (map0 && map0 != MAP_FAILED) munmap((void *)map0, FILE_SIZE);
    close(fd);
    close(fsync_fd);
    map0 = map1 = NULL;
    msleep(100);
    alarm(0);
    mounted = 0;
    if (umount2(MNT, MNT_DETACH) != 0) perror("umount2");
    return 0;
}
