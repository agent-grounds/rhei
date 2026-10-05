/* Observe a genuine zero-byte read; never substitute bytes or errors.
 * Hold the scheduling-thread read until the worker's normal restore has finished.
 * Only the Rhei executable and the fixture's shared task file are affected.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/file.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <fcntl.h>
#include <stdatomic.h>

static _Atomic int used;
static _Atomic long scheduler_tid;

static void mark(const char *root, const char *name) {
    char path[PATH_MAX];
    snprintf(path, sizeof(path), "%s/%s", root, name);
    int fd = syscall(SYS_openat, AT_FDCWD, path, O_WRONLY | O_CREAT, 0600);
    if (fd >= 0) syscall(SYS_close, fd);
}

static int await(const char *root, const char *name, int ticks) {
    char path[PATH_MAX];
    snprintf(path, sizeof(path), "%s/%s", root, name);
    for (int i = 0; i < ticks; i++) {
        if (access(path, F_OK) == 0) return 1;
        usleep(10000);
    }
    return 0;
}

static ssize_t ordinary(int fd, void *buf, size_t count, off_t offset, int positioned) {
    if (positioned) {
        ssize_t (*actual)(int, void *, size_t, off_t) = dlsym(RTLD_NEXT, "pread64");
        return actual(fd, buf, count, offset);
    }
    ssize_t (*actual)(int, void *, size_t) = dlsym(RTLD_NEXT, "read");
    return actual(fd, buf, count);
}

#define actual(fd, buf, count) ordinary(fd, buf, count, offset, positioned)

static ssize_t controlled(int fd, void *buf, size_t count, off_t offset, int positioned) {
    const char *root = getenv("RHEI_REPRO_GATE");
    if (!root || used) return actual(fd, buf, count);
    char exe[PATH_MAX], link[64], path[PATH_MAX];
    ssize_t n = readlink("/proc/self/exe", exe, sizeof(exe) - 1);
    if (n < 0) return actual(fd, buf, count);
    exe[n] = 0;
    const char *base = strrchr(exe, '/');
    if (!base || strcmp(base + 1, "rhei")) return actual(fd, buf, count);
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    n = readlink(link, path, sizeof(path) - 1);
    if (n < 0) return actual(fd, buf, count);
    path[n] = 0;
    const char *suffix = "/tasks/01-shared.md";
    size_t len = strlen(path), tail = strlen(suffix);
    if (len < tail || strcmp(path + len - tail, suffix)) return actual(fd, buf, count);
    /* The CLI may put its scheduler on a thread with a larger stack. Its
     * first startup read happens before any worker exists. */
    long unset = 0, tid = syscall(SYS_gettid);
    atomic_compare_exchange_strong(&scheduler_tid, &unset, tid);
    if (tid != scheduler_tid) return actual(fd, buf, count);
    /* A transition can also reread its just-published completed state. Do
     * not block a reader holding the sidecar that restoration needs. The
     * reported graph comes from an unlocked scheduling checkpoint. */
    char sidecar[PATH_MAX];
    if (len + 6 > sizeof(sidecar)) return actual(fd, buf, count);
    memcpy(sidecar, path, len);
    memcpy(sidecar + len, ".lock", 6);
    int guard = syscall(SYS_openat, AT_FDCWD, sidecar, O_RDWR);
    if (guard < 0) return actual(fd, buf, count);
    int available = syscall(SYS_flock, guard, LOCK_EX | LOCK_NB) == 0;
    if (available) syscall(SYS_flock, guard, LOCK_UN);
    syscall(SYS_close, guard);
    if (!available) return actual(fd, buf, count);
    char preview[4096];
    n = syscall(SYS_pread64, fd, preview, sizeof(preview) - 1, 0);
    if (n <= 0) return actual(fd, buf, count);
    preview[n] = 0;
    if (!strstr(preview, "**State:** completed") || strstr(preview, "#### Visit"))
        return actual(fd, buf, count);
    /* The existing worker polls every 50ms. Allow it to reach its actual
     * truncate-mode open, then order that open ahead of this pending read. */
    if (!await(root, "truncate-ready", 300)) {
        used = 1;
        return actual(fd, buf, count);
    }
    used = 1;
    mark(root, "permit-truncate");
    if (!await(root, "truncated", 1500)) {
        mark(root, "barrier-timeout");
        return actual(fd, buf, count);
    }
    ssize_t got = actual(fd, buf, count);
    struct stat st;
    if (got != 0 || fstat(fd, &st) || st.st_size != 0) {
        mark(root, "unexpected-read");
        mark(root, "empty-captured");
        return got;
    }
    mark(root, "empty-captured");
    dprintf(2, "Repro observation: scheduling reader captured 0 bytes from %s; fd size=0.\n", path);
    if (!await(root, "restore-announced", 1500)) mark(root, "barrier-timeout");
    /* Return the original zero-byte read, even though the pathname is now
     * restored. This is the snapshot a preempted reader actually obtained. */
    return got;
}

ssize_t read(int fd, void *buf, size_t count) {
    return controlled(fd, buf, count, 0, 0);
}

ssize_t pread64(int fd, void *buf, size_t count, off_t offset) {
    return controlled(fd, buf, count, offset, 1);
}
