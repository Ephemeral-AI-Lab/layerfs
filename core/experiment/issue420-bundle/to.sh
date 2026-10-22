#!/bin/sh
# to.sh SECONDS CMD...: run CMD with a hard wall limit (SIGALRM), host side.
s=$1; shift
exec perl -e 'alarm shift; exec @ARGV or die "exec: $!"' "$s" "$@"
