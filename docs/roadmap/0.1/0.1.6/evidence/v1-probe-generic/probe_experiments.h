/* Experiments for probe.c. Writer page order is ASCENDING (page 0 first, page
 * NPAGES-1 last within a round), so at every instant
 *   value(page p) >= value(page q)  for all p < q.
 * A collected sample set with v[p] < v[q] for some p < q therefore mixes two
 * instants: the ascending kernel copy (retrieve / writeback) takes page p at an
 * earlier time than page q, and a writer round boundary between the two copies
 * leaves p holding the older round's value. */

static int main_experiments(void) {
    fprintf(stderr, "probe: argv1=%s (writeback_mode=%d)\n", probe_argv1, writeback_mode);
    CHK(mkdir(MNT, 0755) == 0 || errno == EEXIST);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    if (fuse_fd < 0) { perror("open /dev/fuse"); return 2; }
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d", fuse_fd, S_IFDIR,
             getuid(), getgid());
    if (mount("v1generic", MNT, "fuse", MS_NOSUID | MS_NODEV, opts) != 0) { perror("mount fuse"); return 2; }
    mounted = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t sthread;
    CHK(pthread_create(&sthread, NULL, server, NULL) == 0);

    char path[256], path2[256];
    snprintf(path, sizeof(path), "%s/data", MNT);
    snprintf(path2, sizeof(path2), "%s/data2", MNT);
    int fd = open(path, O_RDWR);
    if (fd < 0) { perror("open data"); return 3; }
    int fd2 = open(path2, O_RDWR);
    if (fd2 < 0) { perror("open data2"); return 3; }
    map0 = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (map0 == MAP_FAILED) { perror("mmap data"); return 3; }
    map1 = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd2, 0);
    if (map1 == MAP_FAILED) { perror("mmap data2"); return 3; }
    printf("probe: page_size=%ld file_size=%u pages=%u MAP_SHARED|PROT_WRITE=accepted\n",
           sysconf(_SC_PAGESIZE), FILE_SIZE, NPAGES);
    printf("probe: negotiated minor=%d flags=0x%x max_write=%u writeback_cache=%u\n",
           negotiated_minor, negotiated_flags, negotiated_max_write,
           (negotiated_flags & FUSE_WRITEBACK_CACHE) ? 1u : 0u);

    /* Pre-fault and dirty every page of data, and page 0 of data2, through the
     * mappings; then quiesce so every page is present and dirty in cache. */
    writer_start(1);
    msleep(200);
    writer_stop_and_join();
    w64(map1, 0, 0x5151515151515151ULL);
    printf("probe: prefault done rounds=%lu daemon_reads=%lu daemon_writes=%lu\n",
           writer_round, read_requests, write_requests);

    unsigned long r0;
    writer_start(1);
    r0 = writer_round;
    msleep(100);
    writer_stop_and_join();
    unsigned long rounds_100ms = writer_round - r0;
    unsigned long ns_per_round = rounds_100ms ? 100000 * 1000 / rounds_100ms : 0;
    printf("probe: writer_rate=%lu rounds/100ms (%lu ns/round, 1 round = %u barriers)\n",
           rounds_100ms, ns_per_round, NPAGES);

    /* ---------- T1: is the retrieved payload a copy, and is it stable? ----- */
    {
        w64(map0, 0, 0x4141414141414141ULL);
        unsigned long wr0 = write_requests;
        uint64_t c = next_cookie++;
        retrieve(NODE_FILE, c, 0, PAGE_SZ);
        CHK(wait_retrieve(c) == 0);
        unsigned long long got;
        memcpy(&got, retrieved, sizeof(got));
        printf("T1 retrieve size=%u reply_word=0x%llx mapped_store_word=0x4141414141414141 "
               "daemon_writes_delta=%lu\n", retrieved_size, got, write_requests - wr0);
        w64(map0, 0, 0x4242424242424242ULL);
        msleep(50);
        unsigned long long again;
        memcpy(&again, retrieved, sizeof(again));
        printf("T1 after_later_mapped_store reply_word=0x%llx reply_word_unchanged=%d "
               "daemon_writes_delta_now=%lu\n", again, again == got, write_requests - wr0);
        printf("RESULT T1 delivered_is_copy_of_current_bytes=%d daemon_copy_stable_after_"
               "mapped_store=%d write_callbacks_for_mapped_stores=%lu\n",
               got == 0x4141414141414141ULL, again == got, write_requests - wr0);
    }

    /* ---------- T2: does notification *return* fix the sampling instant? --- */
    {
        server_park = 1;
        for (int i = 0; i < 5000 && !server_parked; i++) msleep(1);
        CHK(server_parked);
        w64(map0, 0, 0x4343434343434343ULL);        /* content when notify is accepted */
        unsigned long wr0 = write_requests;
        uint64_t c = next_cookie++;
        retrieve(NODE_FILE, c, 0, PAGE_SZ);          /* returns 0: notification accepted */
        w64(map0, 0, 0x4444444444444444ULL);        /* store completed after notify return */
        __sync_synchronize();
        msleep(20);
        server_park = 0;
        CHK(wait_retrieve(c) == 0);
        unsigned long long got;
        memcpy(&got, retrieved, sizeof(got));
        printf("T2 notify_time_word=0x4343434343434343 post_notify_return_store_word=0x4444444444444444 "
               "reply_word=0x%llx daemon_writes_delta=%lu\n", got, write_requests - wr0);
        printf("RESULT T2 reply_bound_to_notification_instant=%d reply_reflects_store_after_return=%d\n",
               got == 0x4343434343434343ULL, got == 0x4444444444444444ULL);
    }

    /* ---------- T3: is one retrieve reply a point-in-time image? ---------- */
    unsigned t3_trials = 0, t3_torn = 0;
    unsigned long long t3_maxdelta = 0;
    {
        writer_start(1);
        msleep(50);
        for (unsigned t = 0; t < 20; t++) {
            uint64_t c = next_cookie++;
            unsigned long rr0 = writer_round;
            retrieve(NODE_FILE, c, 0, NPAGES * PAGE_SZ);
            CHK(wait_retrieve(c) == 0);
            unsigned long rr1 = writer_round;
            unsigned long long v[NPAGES];
            unsigned char present[NPAGES];
            unsigned n = npages_from_retrieved(v, present);
            struct inv iv = check_ordered(v, present, NPAGES);
            t3_trials++;
            if (iv.viol) t3_torn++;
            if (iv.max_delta > t3_maxdelta) t3_maxdelta = iv.max_delta;
            printf("T3 trial=%u pages_returned=%u bytes=%u writer_rounds_during_reply=%lu "
                   "compared_pairs=%lu violating_pairs=%lu max_delta_rounds=%llu\n",
                   t, n, retrieved_size, rr1 - rr0, iv.pairs, iv.viol, iv.max_delta);
        }
        writer_stop_and_join();
        printf("RESULT T3 trials=%u replies_violating_single_instant=%u max_delta_rounds=%llu\n",
               t3_trials, t3_torn, t3_maxdelta);
    }

    /* ---------- T3C: control, quiesced retrieve must satisfy the oracle ---- */
    {
        uint64_t c = next_cookie++;
        retrieve(NODE_FILE, c, 0, NPAGES * PAGE_SZ);
        CHK(wait_retrieve(c) == 0);
        unsigned long long v[NPAGES];
        unsigned char present[NPAGES];
        unsigned n = npages_from_retrieved(v, present);
        struct inv iv = check_ordered(v, present, NPAGES);
        printf("T3C quiesced pages_returned=%u compared_pairs=%lu violating_pairs=%lu max_delta=%llu\n",
               n, iv.pairs, iv.viol, iv.max_delta);
        printf("RESULT T3C quiesced_violations=%lu (control: oracle is satisfiable)\n", iv.viol);
    }

    /* ---------- T4: sequential per-file collection, no common instant ------ */
    unsigned t4_trials = 0, t4_torn = 0;
    unsigned long long t4_maxdelta = 0;
    {
        writer_start(2);
        msleep(50);
        for (unsigned t = 0; t < 20; t++) {
            uint64_t c1 = next_cookie++, c2 = next_cookie++;
            retrieve(NODE_FILE, c1, 0, PAGE_SZ);
            CHK(wait_retrieve(c1) == 0);
            unsigned long long va;
            memcpy(&va, retrieved, sizeof(va));
            retrieve(NODE_FILE2, c2, 0, PAGE_SZ);
            CHK(wait_retrieve(c2) == 0);
            unsigned long long vb;
            memcpy(&vb, retrieved, sizeof(vb));
            int viol = vb > va;
            if (viol) t4_torn++;
            if (vb > va && vb - va > t4_maxdelta) t4_maxdelta = vb - va;
            t4_trials++;
            printf("T4 trial=%u fileA_page0_round=%llu fileB_page0_round=%llu "
                   "collected_delta_rounds=%lld ordering_violation=%d\n",
                   t, va, vb, (long long)vb - (long long)va, viol);
        }
        writer_stop_and_join();
        printf("RESULT T4 trials=%u collected_pairs_violating_single_instant=%u max_delta_rounds=%llu\n",
               t4_trials, t4_torn, t4_maxdelta);
    }

    /* ---------- T5: the daemon's own read(2) through its own mount -------- */
    {
        int self = open(path, O_RDWR);
        if (self < 0) { perror("daemon self-open"); return 5; }
        static unsigned char rbuf[FILE_SIZE];
        writer_start(1);
        msleep(50);
        unsigned long rd0 = read_requests, wr0 = write_requests, fs0 = fsync_requests;
        ssize_t got = pread(self, rbuf, FILE_SIZE, 0);
        unsigned long rd1 = read_requests, wr1 = write_requests, fs1 = fsync_requests;
        unsigned long long v[NPAGES], cur[NPAGES];
        unsigned char present[NPAGES];
        for (unsigned p = 0; p < NPAGES; p++) {
            memcpy(&v[p], rbuf + (size_t)p * PAGE_SZ, sizeof(v[p]));
            memcpy(&cur[p], (const void *)(map0 + (size_t)p * PAGE_SZ), sizeof(cur[p]));
            present[p] = 1;
        }
        struct inv iv = check_ordered(v, present, NPAGES);
        unsigned long long newest_in_sample = 0, current = 0;
        for (unsigned p = 0; p < NPAGES; p++) {
            if (v[p] > newest_in_sample) newest_in_sample = v[p];
            if (cur[p] > current) current = cur[p];
        }
        printf("T5 self_pread_bytes=%zd daemon_read_requests=%lu daemon_write_requests=%lu "
               "daemon_fsync_requests=%lu compared_pairs=%lu violating_pairs=%lu max_delta=%llu\n",
               got, rd1 - rd0, wr1 - wr0, fs1 - fs0, iv.pairs, iv.viol, iv.max_delta);
        printf("T5 sample_oldest=%llu sample_newest=%llu writer_max_after_read=%llu "
               "sample_lags_current_mapping_max=%llu\n",
               v[0], newest_in_sample, current, current > newest_in_sample ? current - newest_in_sample : 0);
        printf("RESULT T5 bytes_visible_without_write_or_fsync_callbacks=%d "
               "collected_is_single_instant=%d write_callbacks=%lu\n",
               got > 0 && (wr1 - wr0) == 0, iv.viol == 0,
               wr1 - wr0);
        writer_stop_and_join();
        close(self);
    }

    /* ---------- T6: daemon-initiated per-file drain under a live writer --- */
    {
        int self = open(path, O_RDWR);
        if (self < 0) { perror("daemon self-open 2"); return 6; }
        writer_start(1);
        msleep(50);
        memset(drain_seen, 0, sizeof(drain_seen));
        memset(drain_val, 0, sizeof(drain_val));
        drain_writes = drain_write_cache = drain_pages = 0;
        unsigned long wr0 = write_requests;
        unsigned long fs0 = fsync_requests;
        unsigned long rr0 = writer_round;
        int rc = fsync(self);
        unsigned long rr1 = writer_round;
        unsigned long wr1 = write_requests, fs1 = fsync_requests;
        unsigned long long v[NPAGES];
        for (unsigned p = 0; p < NPAGES; p++) v[p] = drain_val[p];
        struct inv iv = check_ordered(v, drain_seen, NPAGES);
        unsigned long long cur0, cur0max = 0;
        for (unsigned p = 0; p < NPAGES; p++) {
            memcpy(&cur0, (const void *)(map0 + (size_t)p * PAGE_SZ), sizeof(cur0));
            if (cur0 > cur0max) cur0max = cur0;
        }
        printf("T6 fsync_rc=%d writer_rounds_during_fsync=%lu daemon_write_requests=%lu "
               "write_cache_flagged=%lu pages_delivered=%lu compared_pairs=%lu violating_pairs=%lu "
               "max_delta_rounds=%llu\n",
               rc, rr1 - rr0, wr1 - wr0, drain_write_cache, drain_pages, iv.pairs, iv.viol,
               iv.max_delta);
        printf("T6 drained_page0=%llu drained_pageNminus1=%llu current_mapping_max=%llu "
               "oldest_drained_page0_vs_current=%lld\n",
               drain_seen[0] ? v[0] : 0, drain_seen[NPAGES - 1] ? v[NPAGES - 1] : 0, cur0max,
               (long long)v[0] - (long long)cur0max);
        printf("RESULT T6 fsync_delivered_bytes_to_daemon=%d drained_copy_is_single_instant=%d "
               "fsync_request_before_or_after=%lu/%lu\n",
               (wr1 - wr0) > 0, iv.viol == 0, fs0, fs1);
        writer_stop_and_join();
        close(self);
    }

    printf("probe: done. daemon_totals reads=%lu writes=%lu write_cache=%lu fsync=%lu other=%lu\n",
           read_requests, write_requests, write_cache_requests, fsync_requests, other_requests);
    printf("probe: writeback_cache_flag=%u\n", (negotiated_flags & FUSE_WRITEBACK_CACHE) ? 1u : 0u);
    /* Teardown order matters: unmap the writable mappings and close the fds
     * while the daemon still serves requests, then detach the mount. Exiting
     * with dirty mappings and a live mount left a thread unkillable in the
     * kernel and the container unreapable (observed; see README). */
    if (map1 && map1 != MAP_FAILED) munmap((void *)map1, FILE_SIZE);
    if (map0 && map0 != MAP_FAILED) munmap((void *)map0, FILE_SIZE);
    close(fd2);
    close(fd);
    msleep(100);
    alarm(0);
    mounted = 0;
    if (umount2(MNT, MNT_DETACH) != 0) perror("umount2");
    return 0;
}

int main(int argc, char **argv) {
    struct rlimit rl = {0, 0};
    setrlimit(RLIMIT_CORE, &rl);
    signal(SIGALRM, on_alarm);
    alarm(150);
    if (argc > 1) probe_argv1 = argv[1];
    writeback_mode = (argc > 1 && strcmp(argv[1], "writeback") == 0);
    return main_experiments();
}
