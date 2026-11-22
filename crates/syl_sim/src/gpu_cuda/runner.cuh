// A family occupies one entire block. Every active thread reaches every
// barrier.
__device__ __forceinline__ void barrier() {
#if PARALLEL
  __syncthreads();
#endif
}

__global__ void syl_sim_kernel(Word *state, Word *mem, Word *failure,
                               Word *outputs, unsigned batch, unsigned first,
                               unsigned count) {
  size_t tid = size_t(blockIdx.x) * blockDim.x + threadIdx.x;
  unsigned b = PARALLEL ? tid / FAMILY : tid;
  unsigned lane = PARALLEL ? tid % FAMILY : 0;
  if (b >= batch)
    return;
  Frame f{state, mem, failure, b, batch};
  for (unsigned step = 0; step < count; ++step) {
    unsigned cycle = first + step;
    if (lane == 0) {
      drive(f, cycle);
      clock_level(f, 1);
    }
    barrier();
    eval_comb(f, lane);
    step_clock(f, lane, cycle);
    barrier();
  }
  if (lane == 0)
    clock_level(f, 0);
  barrier();
  eval_comb(f, lane);
  barrier();
  if (lane == 0) {
    gather(f);
    for (unsigned i = 0; i < OUTPUT_WORDS; ++i)
      outputs[size_t(i) * batch + b] = f.word(output_slots[i]);
  }
}

__global__ void checksum(const Word *outputs, size_t size, Word *result) {
  size_t tid = size_t(blockIdx.x) * blockDim.x + threadIdx.x;
  Word v = 0;
  for (size_t i = tid; i < size; i += gridDim.x * blockDim.x)
    v += outputs[i] * (2 * i + 1);
  if (v)
    atomicAdd(result, v);
}
static void check(cudaError_t e, const char *call) {
  if (e != cudaSuccess) {
    std::fprintf(stderr, "%s: %s\n", call, cudaGetErrorString(e));
    std::exit(1);
  }
}
#define CUDA(call) check((call), #call)
struct Timer {
  cudaEvent_t begin, end;
  Timer() {
    CUDA(cudaEventCreate(&begin));
    CUDA(cudaEventCreate(&end));
  }
  void start() { CUDA(cudaEventRecord(begin)); }
  double stop() {
    float ms;
    CUDA(cudaEventRecord(end));
    CUDA(cudaEventSynchronize(end));
    CUDA(cudaEventElapsedTime(&ms, begin, end));
    return ms;
  }
  ~Timer() {
    cudaEventDestroy(begin);
    cudaEventDestroy(end);
  }
};
int main(int argc, char **argv) {
  int count = 0;
  cudaError_t probe = cudaGetDeviceCount(&count);
  if (probe == cudaErrorNoDevice || probe == cudaErrorInsufficientDriver ||
      (probe == cudaSuccess && !count))
    return 77;
  CUDA(probe);
  if (argc == 2 && !std::strcmp(argv[1], "probe"))
    return 0;
  unsigned batch = argc > 1 ? std::strtoul(argv[1], nullptr, 10) : 1;
  unsigned cycles = argc > 2 ? std::strtoul(argv[2], nullptr, 10) : 1000;
  unsigned block = argc > 3 ? std::strtoul(argv[3], nullptr, 10) : 256;
  const char *mode = argc > 4 ? argv[4] : "summary";
  bool each = !std::strcmp(mode, "each") || !std::strcmp(mode, "trace");
  bool dump = !std::strcmp(mode, "trace") || !std::strcmp(mode, "final");
  if (!batch || batch > 0x7fffffff || !cycles || !block || block > 1024 ||
      (~0ull / ASSERT_COUNT) / batch < cycles)
    return 2;
  unsigned threads = PARALLEL ? FAMILY : block;
  unsigned blocks = PARALLEL ? batch : (batch + threads - 1) / threads;
  size_t words = size_t(batch) * OUTPUT_WORDS;
  size_t free0, total, free1;
  CUDA(cudaMemGetInfo(&free0, &total));
  Word *state, *mem, *summary, *outputs, *host = nullptr;
  CUDA(cudaMalloc(&state, 8 * SIGNAL_SLOTS * batch));
  CUDA(cudaMalloc(&mem, 8 * std::max(1ull, MEM_WORDS * batch)));
  CUDA(cudaMalloc(&outputs, 8 * words));
  CUDA(cudaMalloc(&summary, 16));
  if (each || dump)
    CUDA(cudaMallocHost(&host, 8 * words));
  CUDA(cudaMemset(state, 0, 8 * SIGNAL_SLOTS * batch));
  CUDA(cudaMemset(mem, 0, 8 * std::max(1ull, MEM_WORDS * batch)));
  CUDA(cudaDeviceSynchronize());
  Word summary_words[2] = {~0ull, 0};
  Timer t;
  t.start();
  CUDA(cudaMemcpy(summary, summary_words, 16, cudaMemcpyHostToDevice));
  double h2d = t.stop(), kernel = 0, d2h = 0;
  size_t bytes = 16;
  std::vector<Word> samples;
  for (unsigned start = 0; start < cycles;) {
    unsigned n = each ? 1 : cycles;
    t.start();
    syl_sim_kernel<<<blocks, threads>>>(state, mem, summary, outputs, batch,
                                        start + 1, n);
    CUDA(cudaGetLastError());
    kernel += t.stop();
    if (each || dump) {
      t.start();
      CUDA(cudaMemcpyAsync(host, outputs, 8 * words, cudaMemcpyDeviceToHost));
      d2h += t.stop();
      bytes += 8 * words;
      if (dump)
        samples.insert(samples.end(), host, host + words);
    }
    start += n;
  }
  t.start();
  checksum<<<64, 128>>>(outputs, words, summary + 1);
  CUDA(cudaGetLastError());
  double reduction = t.stop();
  t.start();
  CUDA(cudaMemcpy(summary_words, summary, 16, cudaMemcpyDeviceToHost));
  d2h += t.stop();
  CUDA(cudaMemGetInfo(&free1, &total));
  std::printf("{\"batch\":%u,\"cycles\":%u,\"elapsed_ms\":%.6f,\"h2d_ms\":%.6f,"
              "\"d2h_ms\":%.6f,\"reduction_ms\":%.6f,\"d2h_bytes\":%zu,\"peak_"
              "vram_mib\":%zu,\"device_used_mib\":%zu,\"digest\":%llu,",
              batch, cycles, kernel, h2d, d2h, reduction, bytes,
              (free0 > free1 ? free0 - free1 : 0) >> 20, (total - free1) >> 20,
              summary_words[1]);
  Word k = summary_words[0];
  if (k == ~0ull)
    std::printf("\"fail_cycle\":null,\"fail_batch\":null,\"fail_assert\":null");
  else
    std::printf("\"fail_cycle\":%llu,\"fail_batch\":%llu,\"fail_assert\":%llu",
                1 + k / ASSERT_COUNT / batch, k / ASSERT_COUNT % batch,
                k % ASSERT_COUNT);
  std::printf(",\"samples\":[");
  for (size_t i = 0; i < samples.size(); ++i)
    std::printf("%s%llu", i ? "," : "", samples[i]);
  std::puts("]}");
  CUDA(cudaFree(state));
  CUDA(cudaFree(mem));
  CUDA(cudaFree(outputs));
  CUDA(cudaFree(summary));
  if (host)
    CUDA(cudaFreeHost(host));
}
