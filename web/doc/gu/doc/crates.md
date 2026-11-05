# Crate

| સ્તર | Crate | જવાબદારીઓ |
|---|---|---|
| foundation | `fkv_types` | ID, પેટર્ન, સતત |
| foundation | `fkv_error` | ભૂલ મોડલ |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | રૂપરેખાંકન અને INI પદચ્છેદન |
| foundation | `fkv_metrics` | કાઉન્ટર્સ અને હિસ્ટોગ્રામ્સ |
| foundation | `fkv_failpoint` | ફોલ્ટ ઈન્જેક્શન |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | કાચું / શબ્દકોશ સંકોચન |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | બફર સંરેખિત કરો |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | મોટું મૂલ્ય અને સેગમેન્ટ |
| io | `fkv_io` | અસુમેળ IO લક્ષણો |
| io | `fkv_io_compio` | કમ્પિયો બેકએન્ડ |
| io | `fkv_io_mem` | મેમરી બેકએન્ડ |
| io | `fkv_io_sim` | સિમ્યુલેશન બેકએન્ડ |
| io | `fkv_object` | ઑબ્જેક્ટ સ્ટોરેજ લક્ષણો |
| io | `fkv_object_s3` | S3 બેકએન્ડ |
| io | `fkv_standby` | સ્ટેન્ડબાય નકલ |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | વર્ક લૂપ અને એક્સેસ |
| engine | `fkv_store` | સ્ટોર જીવન ચક્ર અને રૂટીંગ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ફરીથી ચલાવો અને ફરીથી ખોલો |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | એન્ડ-ટુ-એન્ડ બેન્ચમાર્ક |