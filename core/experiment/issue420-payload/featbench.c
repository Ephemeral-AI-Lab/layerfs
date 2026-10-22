// #420 payload study (question C): native cost of the resemblance features of features.py.
//
// usage: featbench FILE MAXBYTES CHUNK
// Reads up to MAXBYTES of FILE (a raw payload dump), cuts it into CHUNK-byte pieces and computes for
// each piece: (a) full: 32-byte polynomial rolling hash at every position, 12 transforms, max each
// (features.py); (b) sampled: the same, but only positions whose hash has its low 5 bits zero
// (1/32 sampling, Finesse/Odess style). Prints ns per KB for both. One run; -O2; single thread.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define W 32
static uint64_t A[12], B[12];

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e9 + t.tv_nsec;
}

static uint64_t run(const uint8_t *p, size_t n, int sampled, uint64_t *out) {
    const uint64_t R = 0x100000001B3ULL;
    uint64_t rw = 1;
    for (int i = 0; i < W - 1; i++) rw *= R;  // R^(W-1)
    for (int i = 0; i < 12; i++) out[i] = 0;
    if (n < 64) return 0;
    uint64_t h = 0;
    for (int i = 0; i < W; i++) h = h * R + p[i];
    uint64_t taken = 0;
    for (size_t i = 0;; i++) {
        if (!sampled || (h & 31) == 0) {
            taken++;
            for (int k = 0; k < 12; k++) {
                uint64_t v = h * A[k] + B[k];
                if (v > out[k]) out[k] = v;
            }
        }
        if (i + W >= n) break;
        h = (h - p[i] * rw) * R + p[i + W];
    }
    return taken;
}

int main(int argc, char **argv) {
    if (argc < 4) return 2;
    FILE *f = fopen(argv[1], "rb");
    size_t max = strtoull(argv[2], 0, 10), chunk = strtoull(argv[3], 0, 10);
    uint8_t *buf = malloc(max);
    size_t n = fread(buf, 1, max, f);
    fclose(f);
    uint64_t s = 420;
    for (int k = 0; k < 12; k++) {
        s = s * 6364136223846793005ULL + 1442695040888963407ULL;
        A[k] = s | 1;
        s = s * 6364136223846793005ULL + 1442695040888963407ULL;
        B[k] = s;
    }
    uint64_t out[12], sink = 0;
    for (int mode = 0; mode < 2; mode++) {
        double t0 = now();
        for (size_t off = 0; off + chunk <= n; off += chunk) {
            run(buf + off, chunk, mode, out);
            sink ^= out[0];
        }
        double t1 = now();
        printf("{\"mode\":\"%s\",\"bytes\":%zu,\"chunk\":%zu,\"ns_per_kb\":%.1f}\n", mode ? "sampled" : "full",
               n, chunk, (t1 - t0) / (n / 1024.0));
    }
    fprintf(stderr, "sink %llu\n", (unsigned long long)sink);
    return 0;
}
