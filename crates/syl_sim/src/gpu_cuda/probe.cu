#include <cstdio>
#include <cuda_runtime.h>

// No device code. This probe can be built before choosing a kernel target.
int main() {
  int count;
  cudaDeviceProp prop;
  if (cudaGetDeviceCount(&count) != cudaSuccess || !count ||
      cudaGetDeviceProperties(&prop, 0) != cudaSuccess ||
      cudaFree(nullptr) != cudaSuccess)
    return 77;
  std::printf("sm_%d%d\n", prop.major, prop.minor);
}
