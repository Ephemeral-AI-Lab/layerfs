source <scratch>/env.sh
export LFS533_HOME=$HD LFS535_PG_PASSWORD=env:LAYERFS_PG_L1
LFS535_PG_URL='postgres://postgres@127.0.0.1:55551/{cell}' bash $R/series.sh docker pg-loop $R/logs/series-docker-pg-loop.log C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
LFS535_PG_URL='postgres://postgres@192.168.2.123:55551/r{cell}' bash $R/series.sh docker pg-remote $R/logs/series-docker-pg-remote.log C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
