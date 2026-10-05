/* Pause a real read of completed state before the finalizer writes its link.
 * Destination bytes and read results are never substituted. Only a reader
 * observing the engine's held sidecar can enter this observation window.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/syscall.h>
#include <unistd.h>

static _Atomic int used;

static void snapshot(int fd, ssize_t got) {
    const char *root = getenv("RHEI_REPRO_GATE");
    if (!root || used || got <= 0) return;
    char exe[PATH_MAX], link[64], path[PATH_MAX];
    ssize_t n = readlink("/proc/self/exe", exe, sizeof(exe) - 1);
    if (n < 0) return;
    exe[n] = 0;
    const char *base = strrchr(exe, '/');
    if (!base || strcmp(base + 1, "rhei")) return;
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    n = readlink(link, path, sizeof(path) - 1);
    if (n < 0) return;
    path[n] = 0;
    const char *suffix = "/tasks/01-shared.md";
    size_t len = strlen(path), tail = strlen(suffix);
    if (len < tail || strcmp(path + len - tail, suffix)) return;
    char sidecar[PATH_MAX];
    if (len + 6 > sizeof(sidecar)) return;
    memcpy(sidecar, path, len);
    memcpy(sidecar + len, ".lock", 6);
    int guard = syscall(SYS_openat, AT_FDCWD, sidecar, O_RDWR);
    if (guard < 0) return;
    int available = syscall(SYS_flock, guard, LOCK_EX | LOCK_NB) == 0;
    if (available) syscall(SYS_flock, guard, LOCK_UN);
    syscall(SYS_close, guard);
    if (available) return;
    char text[4096];
    n = syscall(SYS_pread64, fd, text, sizeof(text) - 1, 0);
    if (n <= 0) return;
    text[n] = 0;
    char *sibling = strstr(text, "### Task 2:");
    if (!sibling || !strstr(sibling, "**State:** completed") ||
        strstr(sibling, "> **Result:**")) return;
    int unset = 0;
    if (!atomic_compare_exchange_strong(&used, &unset, 1)) return;
    snprintf(path, sizeof(path), "%s/state-published", root);
    guard = syscall(SYS_openat, AT_FDCWD, path, O_WRONLY | O_CREAT, 0600);
    if (guard >= 0) syscall(SYS_close, guard);
    dprintf(2, "Repro observation: sibling state published before its result link; sidecar held.\n");
    snprintf(path, sizeof(path), "%s/snapshot-held", root);
    /* A synchronized publisher waits for the held writer rather than reading
     * an intermediate image. Release after a bounded observation window so
     * that correct exclusion can finish; old code releases on its snapshot. */
    for (int i = 0; i < 300; i++) {
        if (access(path, F_OK) == 0) break;
        usleep(10000);
    }
}

ssize_t read(int fd, void *buf, size_t count) {
    ssize_t (*ordinary)(int, void *, size_t) = dlsym(RTLD_NEXT, "read");
    ssize_t got = ordinary(fd, buf, count);
    snapshot(fd, got);
    return got;
}

ssize_t pread64(int fd, void *buf, size_t count, off_t offset) {
    ssize_t (*ordinary)(int, void *, size_t, off_t) = dlsym(RTLD_NEXT, "pread64");
    ssize_t got = ordinary(fd, buf, count, offset);
    snapshot(fd, got);
    return got;
}
