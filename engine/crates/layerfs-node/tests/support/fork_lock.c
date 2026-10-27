/* Test-only fork at a real discovery lock acquisition. The child retains the
 * open file description while the public binding call drops its Guard. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <unistd.h>

int flock(int fd, int operation) {
    int (*original)(int, int) = dlsym(RTLD_NEXT, "flock");
    int result = original(fd, operation);
    const char *home = getenv("LAYERFS_FORK_HOME");
    static int forked;
    if (result == 0 && operation == (LOCK_EX | LOCK_NB) && home && !forked) {
        char link[64], target[4096], expected[4096];
        snprintf(link, sizeof link, "/proc/self/fd/%d", fd);
        ssize_t size = readlink(link, target, sizeof target - 1);
        if (size < 0) return result;
        target[size] = 0;
        snprintf(expected, sizeof expected, "%s/proj/ownership.lock", home);
        if (strcmp(target, expected) != 0) return result;
        forked = 1;
        pid_t child = fork();
        if (child < 0) _exit(90);
        if (child == 0) {
            close(0); close(1); close(2);
            char release[4096];
            snprintf(release, sizeof release, "%s/release", home);
            for (int i = 0; i < 1000 && access(release, F_OK) != 0; ++i) usleep(10000);
            snprintf(release, sizeof release, "%s/fork.done", home);
            int done = open(release, O_WRONLY | O_CREAT, 0600);
            if (done >= 0) close(done);
            _exit(0);
        }
        char pidfile[4096];
        snprintf(pidfile, sizeof pidfile, "%s/fork.pid", home);
        FILE *output = fopen(pidfile, "w");
        if (!output) _exit(91);
        fprintf(output, "%d", child);
        fclose(output);
    }
    return result;
}
