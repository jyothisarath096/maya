
#define MAX_HEADERS 8
#define MAX_LINE    128
#define BUF_SIZE    512

typedef struct {
    char method[16];
    char path[64];
    char headers[MAX_HEADERS][MAX_LINE];
    int  header_count;
    int  content_length;
    int  status;
} HttpRequest;

__attribute__((noinline)) int str_len(const char *s) {
    int n = 0; while (s[n]) n++; return n;
}

__attribute__((noinline)) int str_eq(const char *a, const char *b, int n) {
    for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1;
}

__attribute__((noinline)) void str_copy(char *dst, const char *src, int max) {
    int i = 0;
    while (i < max-1 && src[i]) { dst[i] = src[i]; i++; }
    dst[i] = 0;
}

__attribute__((noinline)) void buf_fill(char *buf, int size) {
    const char *t = "GET /api/v1/data HTTP/1.1\r\nHost: maya.os\r\nContent-Length: 42\r\n\r\n";
    int tlen = str_len(t);
    for (int i = 0; i < size; i++) buf[i] = t[i % tlen];
}

__attribute__((noinline)) int parse_method(HttpRequest *r, const char *buf) {
    int i = 0;
    while (buf[i] && buf[i] != ' ' && i < 15) { r->method[i] = buf[i]; i++; }
    r->method[i] = 0; return i;
}

__attribute__((noinline)) int parse_path(HttpRequest *r, const char *buf) {
    int i = 0;
    while (buf[i] && buf[i] != ' ' && i < 63) { r->path[i] = buf[i]; i++; }
    r->path[i] = 0; return i;
}

__attribute__((noinline)) int parse_header_line(HttpRequest *r, const char *line, int idx) {
    if (idx >= MAX_HEADERS) return 0;
    str_copy(r->headers[idx], line, MAX_LINE);
    if (str_eq(line, "Content-Length:", 15)) {
        int val = 0, i = 16;
        while (line[i] >= '0' && line[i] <= '9') { val = val*10 + (line[i]-'0'); i++; }
        r->content_length = val;
    }
    return 1;
}

__attribute__((noinline)) void parse_request(HttpRequest *r, const char *buf) {
    r->header_count = 0; r->content_length = 0; r->status = 0;
    int pos = parse_method(r, buf); pos++;
    pos += parse_path(r, buf + pos);
    while (buf[pos] && buf[pos] != '\n') pos++; pos++;
    char line[MAX_LINE]; int lidx = 0;
    while (buf[pos] && r->header_count < MAX_HEADERS) {
        if (buf[pos] == '\r' || buf[pos] == '\n') {
            if (lidx > 0) { line[lidx]=0; parse_header_line(r,line,r->header_count++); lidx=0; }
            pos++; continue;
        }
        if (lidx < MAX_LINE-1) line[lidx++] = buf[pos]; pos++;
    }
    r->status = 200;
}

__attribute__((noinline)) int validate_request(HttpRequest *r) {
    if (str_len(r->method) == 0) return 0;
    if (str_len(r->path) == 0) return 0;
    if (r->status != 200) return 0;
    return 1;
}

__attribute__((noinline)) void io_loop(void) {
    HttpRequest req;
    char buffer[BUF_SIZE];
    buf_fill(buffer, BUF_SIZE);
    parse_request(&req, buffer);
    validate_request(&req);
}

void _start(void) {
    while (1) {
        io_loop();
        register long x8 __asm__("x8") = 0x01;
        __asm__ volatile("svc #0" : : "r"(x8));
    }
}
