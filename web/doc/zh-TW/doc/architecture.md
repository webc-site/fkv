# 架構

## 運行時

每個 shard 獨佔一個 OS 線程與一個 compio runtime。請求經 ingress 隊列進入，
由 shard 狀態機准入，透過 oneshot 回執完成。

## 儲存

引擎使用 copy-on-write B+tree。 data page、index page、overflow chain 與
manifest 序列化進 segment 檔。 branch 由 `(branch, term)` 標識，各自持有
manifest 與 mapping snapshot。

## IO

`fkv_io` 定義非同步 IO trait。 `fkv_io_compio` 用 compio + io_uring 實現，
`fkv_io_mem` / `fkv_io_sim` 提供記憶體與故障注入後端，`fkv_object_s3` 提供
S3 物件後端。

## 持久化

manifest 記錄 root page、TTL root、branch metadata 與 mapping。 Reopen 時
先 replay manifest，再重建 page mapper，之後才開始服務請求。