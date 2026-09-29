#define ZSTD_STATIC_LINKING_ONLY
#include <zstd.h>
#include <stdio.h>
int main(void) {
  for (int level=9; level<=12; level+=3) {
    for (int raw=131071; raw<=1048576; raw=raw==131071?1000000:1048577) {
      ZSTD_compressionParameters p=ZSTD_getCParams(level,raw,raw);
      printf("level=%d raw=%d dictHint=%d workspace=%zu windowLog=%u\n",level,raw,raw,ZSTD_estimateCCtxSize_usingCParams(p),p.windowLog);
    }
  }
  return 0;
}
