/* Linux-only storage faults adapted from rhei.73's enospc.c; root fails too.
 * Only record publication fails, after the worker sets its flag.
 * §FS-rhei-agents.8.4 §FS-rhei-run.3 */
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
#include <unistd.h>

static int fail(const char *path, const char *operation) {
    const char *root = getenv("SPAWN_FAULT_ROOT");
    const char *mode = getenv("SPAWN_FAULT_MODE");
    if (!root || !mode || strcmp(mode, operation)) return 0;
    if (strncmp(path, root, strlen(root)) || !strstr(path, "/runtime/spawns/")) return 0;
    char flag[PATH_MAX];
    snprintf(flag, sizeof flag, "%s/fault-active", root);
    if (access(flag, F_OK)) return 0;
    /* Prove the intended syscall was reached, independently of diagnostics. */
    snprintf(flag, sizeof flag, "%s/fault-hit", root);
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    int fd = real_open(flag, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd >= 0) { real_write(fd, operation, strlen(operation)); close(fd); }
    errno = ENOSPC;
    return 1;
}

ssize_t write(int fd, const void *buf, size_t n) {
    ssize_t (*real)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    char link[64], path[PATH_MAX];
    snprintf(link, sizeof link, "/proc/self/fd/%d", fd);
    ssize_t len = readlink(link, path, sizeof path - 1);
    if (len > 0) {
        path[len] = 0;
        if (fail(path, "write")) return -1;
    }
    ssize_t written = real(fd, buf, n);
    if (written > 0 && len > 0 && fail(path, "replace")) {
        /* The transaction already read the absent destination. Obstruct only
         * the subsequent rename, which rustix may issue without libc. */
        char *slash = strrchr(path, '/');
        if (slash) {
            strcpy(slash + 1, "task-plan.1-work.json");
            mkdir(path, 0700);
        }
    }
    return written;
}

int open64(const char *path, int flags, ...) {
    int (*real)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, int);
        va_end(args);
        if (fail(path, "stage")) return -1;
    }
    return real(path, flags, mode);
}
