#!/bin/sh
# usage: tmo.sh SECONDS cmd... — run with a hard wall limit (SIGALRM)
s=$1; shift; exec perl -e 'alarm shift; exec @ARGV or die' "$s" "$@"
