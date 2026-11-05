# Crate

| 層 | Crate | 責任 |
|---|---|---|
| foundation | `fkv_types` | ID、パターン、定数 |
| foundation | `fkv_error` | エラーモデル |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | 構成とINI解析 |
| foundation | `fkv_metrics` | カウンタとヒストグラム |
| foundation | `fkv_failpoint` | フォールトインジェクション |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | 生/辞書圧縮 |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | バッファを整列させる |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | 大きな値とセグメント |
| io | `fkv_io` | 非同期 IO 特性 |
| io | `fkv_io_compio` | コンピオバックエンド |
| io | `fkv_io_mem` | メモリバックエンド |
| io | `fkv_io_sim` | シミュレーションバックエンド |
| io | `fkv_object` | オブジェクトストレージの特性 |
| io | `fkv_object_s3` | S3 バックエンド |
| io | `fkv_standby` | スタンバイコピー |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ワークループとアクセス |
| engine | `fkv_store` | ストアのライフサイクルとルーティング |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | 再生して再度開く |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | エンドツーエンドのベンチマーク |