
#define N 64

static int data[N];
static int temp[N];

__attribute__((noinline)) void array_init(int *arr, int n, int seed) {
    for (int i = 0; i < n; i++)
        arr[i] = (seed * 1103515245 + i * 12345) & 0x7fff;
}

__attribute__((noinline)) void bubble_sort(int *arr, int n) {
    for (int i = 0; i < n-1; i++)
        for (int j = 0; j < n-i-1; j++)
            if (arr[j] > arr[j+1]) {
                int t = arr[j]; arr[j] = arr[j+1]; arr[j+1] = t;
            }
}

__attribute__((noinline)) void merge(int *arr, int l, int m, int r) {
    int i = l, j = m+1, k = l;
    while (i <= m && j <= r)
        temp[k++] = (arr[i] <= arr[j]) ? arr[i++] : arr[j++];
    while (i <= m) temp[k++] = arr[i++];
    while (j <= r) temp[k++] = arr[j++];
    for (i = l; i <= r; i++) arr[i] = temp[i];
}

__attribute__((noinline)) void merge_sort(int *arr, int l, int r) {
    if (l >= r) return;
    int m = (l + r) / 2;
    merge_sort(arr, l, m);
    merge_sort(arr, m+1, r);
    merge(arr, l, m, r);
}

__attribute__((noinline)) int binary_search(int *arr, int n, int target) {
    int lo = 0, hi = n-1;
    while (lo <= hi) {
        int mid = (lo + hi) / 2;
        if (arr[mid] == target) return mid;
        if (arr[mid] < target) lo = mid+1;
        else hi = mid-1;
    }
    return -1;
}

__attribute__((noinline)) int array_sum(int *arr, int n) {
    int s = 0;
    for (int i = 0; i < n; i++) s += arr[i];
    return s;
}

__attribute__((noinline)) int array_max(int *arr, int n) {
    int m = arr[0];
    for (int i = 1; i < n; i++) if (arr[i] > m) m = arr[i];
    return m;
}

__attribute__((noinline)) void compute_loop(void) {
    array_init(data, N, 42);
    bubble_sort(data, N/2);
    array_init(data, N, 99);
    merge_sort(data, 0, N-1);
    binary_search(data, N, array_max(data, N)/2);
    array_sum(data, N);
}

void _start(void) {
    while (1) {
        compute_loop();
        register long x8 __asm__("x8") = 0x01;
        __asm__ volatile("svc #0" : : "r"(x8));
    }
}
