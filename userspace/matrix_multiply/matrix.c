#define N 8

static int mat_a[N][N];
static int mat_b[N][N];
static int mat_c[N][N];

void matrix_init(int mat[N][N], int seed) {
    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++)
            mat[i][j] = (seed + i * N + j) % 16;
}

void matrix_multiply(int a[N][N], int b[N][N], int c[N][N]) {
    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            c[i][j] = 0;
            for (int k = 0; k < N; k++)
                c[i][j] += a[i][k] * b[k][j];
        }
}

int matrix_sum(int mat[N][N]) {
    int sum = 0;
    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++)
            sum += mat[i][j];
    return sum;
}

void compute_loop(void) {
    matrix_init(mat_a, 1);
    matrix_init(mat_b, 2);
    matrix_multiply(mat_a, mat_b, mat_c);
    matrix_sum(mat_c);
}

void _start(void) {
    while (1) {
        compute_loop();
        // SYS_YIELD
        register long x8 __asm__("x8") = 0x01;
        __asm__ volatile("svc #0" : : "r"(x8));
    }
}
