/* Controlled publication schedule for #702. §FS-rhei-transition-cmd.3.4
 * §REQ-cross-platform.4.1 */
/* Linux-only scheduling instrumentation. It changes no bytes and delays only
 * the test harness's in-place replacement of an existing fixture machine. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/file.h>
#include <time.h>
#include <unistd.h>

static void wait_for_run_exit(const char *path, int (*real_open)(const char *, int, ...)) {
    char lock_path[PATH_MAX];
    size_t parent_len = (size_t)(strrchr(path, '/') - path);
    if (parent_len + sizeof("/.rhei/run.lock") > sizeof(lock_path)) {
        fprintf(stderr, "RHEI702: instrumentation path too long\n");
        return;
    }
    memcpy(lock_path, path, parent_len);
    strcpy(lock_path + parent_len, "/.rhei/run.lock");
    int lock = real_open(lock_path, O_RDONLY);
    if (lock < 0) {
        fprintf(stderr, "RHEI702: cannot inspect run lock: %s\n", strerror(errno));
        return;
    }
    /* The writer has not written the replacement yet. Wait for the scheduler
     * to sample the empty document and release its lock, rather than betting
     * on the duration of a slow runner's scheduling interval. */
    for (int tick = 0; tick < 2400; ++tick) {
        if (!flock(lock, LOCK_EX | LOCK_NB)) {
            fprintf(stderr, "RHEI702: run lock FREE before replacement write and before transition: %s\n", lock_path);
            flock(lock, LOCK_UN);
            close(lock);
            if (!strcmp(getenv("RHEI702_DELAY"), "project")) {
                /* The obstructed project's descriptor lives one root above
                 * this fixture's member. Probe that root independently too. */
                size_t project_len = parent_len - strlen("/ticket");
                strcpy(lock_path + project_len, "/.rhei/run.lock");
                int project_lock = real_open(lock_path, O_RDONLY);
                if (project_lock >= 0) {
                    if (!flock(project_lock, LOCK_EX | LOCK_NB)) {
                        fprintf(stderr, "RHEI702: project run lock FREE before replacement write and before transition: %s\n", lock_path);
                        flock(project_lock, LOCK_UN);
                    }
                    close(project_lock);
                }
            }
            return;
        }
        if (errno != EWOULDBLOCK && errno != EAGAIN) break;
        if (!tick) fprintf(stderr, "RHEI702: run lock HELD at truncation: %s\n", lock_path);
        struct timespec left = { .tv_sec = 0, .tv_nsec = 25000000 };
        while (nanosleep(&left, &left) && errno == EINTR) {}
    }
    fprintf(stderr, "RHEI702: could not observe run lock release within 60s\n");
    close(lock);
}

static int delayed_open(const char *path, int flags, mode_t mode, const char *symbol) {
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, symbol);
    const char *which = getenv("RHEI702_DELAY");
    struct stat before;
    int inject = which && (flags & O_TRUNC) && strstr(path, "issue-653-live-gate-budget")
        && strlen(path) >= 12 && !strcmp(path + strlen(path) - 12, "/states.yaml")
        && !stat(path, &before) && before.st_size > 0;
    if (inject) {
        int project = strstr(path, "/ticket/states.yaml") != NULL;
        inject = project == !strcmp(which, "project");
    }
    int fd = real_open(path, flags, mode);
    if (fd >= 0 && inject) {
        fprintf(stderr, "RHEI702: opened existing machine with O_TRUNC; delaying its write: %s\n", path);
        fflush(stderr);
        wait_for_run_exit(path, real_open);
    }
    return fd;
}

int open64(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }
    return delayed_open(path, flags, mode, "open64");
}

int open(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }
    return delayed_open(path, flags, mode, "open");
}
