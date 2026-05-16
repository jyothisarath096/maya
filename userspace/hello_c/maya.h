#ifndef MAYA_H
#define MAYA_H

typedef long long i64;
typedef unsigned long long u64;

#define SYS_EXIT   0
#define SYS_WRITE  1
#define SYS_READ   2
#define SYS_GETCAP 5

#define CAP_STDOUT 0
#define CAP_STDIN  1

static inline i64 syscall1(u64 nr, u64 a1) {
    i64 ret;
    __asm__ volatile (
        "syscall"
        : "=a"(ret)
        : "0"(nr), "D"(a1)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static inline i64 syscall3(u64 nr,
    u64 a1, u64 a2, u64 a3) {
    i64 ret;
    __asm__ volatile (
        "syscall"
        : "=a"(ret)
        : "0"(nr), "D"(a1), "S"(a2), "d"(a3)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static inline void maya_exit(int code) {
    syscall1(SYS_EXIT, (u64)code);
    __builtin_unreachable();
}

static inline i64 maya_write(
    u64 cap, const char *buf, u64 len) {
    return syscall3(SYS_WRITE, cap,
        (u64)buf, len);
}

static inline u64 maya_getcap(u64 id) {
    return (u64)syscall1(SYS_GETCAP, id);
}

static inline u64 maya_strlen(const char *s) {
    u64 n = 0;
    while (s[n]) n++;
    return n;
}

#define maya_print(s) do { \
    u64 cap = maya_getcap(CAP_STDOUT); \
    maya_write(cap, s, maya_strlen(s)); \
} while(0)

#endif
