#define _GNU_SOURCE
#include <sys/resource.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
int main(int argc,char **argv) {
 if(argc!=3) return 2;
 int pid=atoi(argv[1]);
 rlim_t size=strcmp(argv[2],"max")==0 ? RLIM_INFINITY : (rlim_t)strtoull(argv[2],0,10);
 struct rlimit old, value={size,RLIM_INFINITY};
 if(syscall(SYS_prlimit64,pid,RLIMIT_FSIZE,&value,&old)!=0) {perror("prlimit64");return 1;}
 printf("old=%llu new=%llu\n",(unsigned long long)old.rlim_cur,(unsigned long long)size);
 return 0;
}
