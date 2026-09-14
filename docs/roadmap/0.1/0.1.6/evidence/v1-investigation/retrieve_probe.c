/* Changed-mechanism probe: reuse the original cached FUSE server and intercept
 * NOTIFY_REPLY. No product source, benchmark, or original evidence is changed. */
#define _GNU_SOURCE
#include <assert.h>
#include <poll.h>
#include <stdatomic.h>
#include <sys/types.h>
#include <unistd.h>

static ssize_t probe_read(int fd, void *buf, size_t count);
#define read probe_read
#define main original_probe_main
#include "../v1-probe/probe.c"
#undef main
#undef read

static atomic_int read_paused, pause_reads, notify_count, writes_seen;
static unsigned char retrieved[FILE_SIZE];
static uint32_t retrieved_size;
static uint64_t retrieved_cookie;

/* Pause before read(2), not after it: FUSE copies page contents during read.
 * Pausing is only an adversarial probe scheduler; it is not a product design. */
static ssize_t probe_read(int fd, void *buf, size_t count) {
    for (;;) {
        while (atomic_load(&pause_reads)) {
            atomic_store(&read_paused, 1);
            msleep(1);
        }
        atomic_store(&read_paused, 0);
        struct pollfd pfd = {.fd = fd, .events = POLLIN};
        int ready = poll(&pfd, 1, 10);
        if (ready < 0) return -1;
        if (!ready || atomic_load(&pause_reads)) continue;
        ssize_t len = read(fd, buf, count);
        if (len < (ssize_t)sizeof(struct fuse_in_header)) return len;
        const struct fuse_in_header *header = buf;
        if (header->opcode == FUSE_WRITE) atomic_fetch_add(&writes_seen, 1);
        if (header->opcode != FUSE_NOTIFY_REPLY) return len;
        const struct fuse_notify_retrieve_in *result =
            (const void *)((const unsigned char *)buf + sizeof(*header));
        assert(len >= (ssize_t)(sizeof(*header) + sizeof(*result)));
        assert(result->size <= sizeof(retrieved));
        assert((size_t)len == sizeof(*header) + sizeof(*result) + result->size);
        memcpy(retrieved, result + 1, result->size);
        retrieved_size = result->size;
        retrieved_cookie = header->unique;
        atomic_fetch_add(&notify_count, 1);
        /* FUSE_NOTIFY_REPLY has no daemon reply. */
    }
}

static void wait_at_least(atomic_int *value, int target) {
    for (int tries = 0; tries < 2000; tries++) {
        if (atomic_load(value) >= target) return;
        msleep(1);
    }
    fprintf(stderr, "probe: timeout waiting for target=%d actual=%d\n",
            target, atomic_load(value));
    abort();
}

static void retrieve(uint64_t nodeid, uint64_t cookie) {
    struct fuse_notify_retrieve_out request = {
        .notify_unique = cookie, .nodeid = nodeid, .offset = 0, .size = FILE_SIZE,
    };
    assert(reply(0, FUSE_NOTIFY_RETRIEVE, &request, sizeof(request)) == 0);
}

int main(void) {
    const char *mnt = "/mnt/v1retrieve";
    assert(mkdir(mnt, 0755) == 0 || errno == EEXIST);
    fuse_fd = open("/dev/fuse", O_RDWR | O_CLOEXEC);
    assert(fuse_fd >= 0);
    char opts[256];
    snprintf(opts, sizeof(opts), "fd=%d,rootmode=%o,user_id=%d,group_id=%d",
             fuse_fd, S_IFDIR, getuid(), getgid());
    assert(mount("v1retrieve", mnt, "fuse", MS_NOSUID | MS_NODEV, opts) == 0);
    setvbuf(stdout, NULL, _IONBF, 0);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, server, NULL) == 0);
    int fd = open("/mnt/v1retrieve/data", O_RDWR);
    assert(fd >= 0);
    volatile unsigned char *mapping =
        mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    assert(mapping != MAP_FAILED);
    printf("probe: negotiated minor=%u flags=0x%x max_write=%u\n",
           negotiated_minor, negotiated_flags, negotiated_max_write);

    mapping[0] = 'A';
    atomic_thread_fence(memory_order_seq_cst);
    retrieve(NODE_FILE, 101);
    wait_at_least(&notify_count, 1);
    printf("probe: H1 dirty retrieve cookie=%lu size=%u byte=%u writes=%d\n",
           retrieved_cookie, retrieved_size, retrieved[0], atomic_load(&writes_seen));
    assert(retrieved_cookie == 101 && retrieved_size == FILE_SIZE);
    assert(retrieved[0] == 'A' && atomic_load(&writes_seen) == 0);

    atomic_store(&pause_reads, 1);
    wait_at_least(&read_paused, 1);
    retrieve(NODE_FILE, 102); /* Kernel has retained page refs when this returns. */
    mapping[0] = 'B';
    atomic_thread_fence(memory_order_seq_cst);
    printf("probe: H2 notify returned; later mapped store completed before daemon read\n");
    atomic_store(&pause_reads, 0);
    wait_at_least(&notify_count, 2);
    printf("probe: H2 retained page cookie=%lu size=%u byte=%u writes=%d "
           "notification_time_byte=65 later_store_byte=66\n",
           retrieved_cookie, retrieved_size, retrieved[0], atomic_load(&writes_seen));
    assert(retrieved_cookie == 102 && retrieved_size == FILE_SIZE);
    assert(retrieved[0] == 'B' && atomic_load(&writes_seen) == 0);

    int fd2 = open("/mnt/v1retrieve/data2", O_RDWR);
    assert(fd2 >= 0);
    volatile unsigned char *mapping2 =
        mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd2, 0);
    assert(mapping2 != MAP_FAILED);
    mapping2[0] = 'C';
    atomic_thread_fence(memory_order_seq_cst);
    assert(unlink("/mnt/v1retrieve/data2") == 0);
    retrieve(NODE_FILE2, 103);
    wait_at_least(&notify_count, 3);
    printf("probe: H3 open-unlinked retrieve cookie=%lu size=%u byte=%u writes=%d\n",
           retrieved_cookie, retrieved_size, retrieved[0], atomic_load(&writes_seen));
    assert(retrieved_cookie == 103 && retrieved_size == FILE_SIZE);
    assert(retrieved[0] == 'C' && atomic_load(&writes_seen) == 0);
    printf("probe: all mechanism assertions passed; V1 snapshot acceptance remains OPEN\n");

    /* Unmapping may flush; all observations above precede cleanup. */
    assert(munmap((void *)mapping2, FILE_SIZE) == 0);
    assert(close(fd2) == 0);
    assert(munmap((void *)mapping, FILE_SIZE) == 0);
    assert(close(fd) == 0);
    assert(umount2(mnt, MNT_DETACH) == 0);
    return 0;
}
