# Crate

| ស្រទាប់ | Crate | ទំនួលខុសត្រូវ |
|---|---|---|
| foundation | `fkv_types` | លេខសម្គាល់, លំនាំ, ថេរ |
| foundation | `fkv_error` | គំរូកំហុស |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | ការកំណត់រចនាសម្ព័ន្ធ និងការញែក INI |
| foundation | `fkv_metrics` | បញ្ជរ និងអ៊ីស្តូក្រាម |
| foundation | `fkv_failpoint` | ការចាក់ថ្នាំខុស |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ឆៅ / ការបង្ហាប់វចនានុក្រម |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | តម្រឹមសតិបណ្ដោះអាសន្ន |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | តម្លៃធំនិងផ្នែក |
| io | `fkv_io` | លក្ខណៈ IO អសមកាល |
| io | `fkv_io_compio` | ផ្នែកខាងក្រោយ compio |
| io | `fkv_io_mem` | ផ្នែកខាងក្រោយនៃការចងចាំ |
| io | `fkv_io_sim` | ផ្នែកខាងក្រោយនៃការក្លែងធ្វើ |
| io | `fkv_object` | លក្ខណៈនៃការផ្ទុកវត្ថុ |
| io | `fkv_object_s3` | ផ្នែកខាងក្រោយ S3 |
| io | `fkv_standby` | ច្បាប់ចម្លងរង់ចាំ |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | រង្វិលជុំការងារ និងការចូលប្រើ |
| engine | `fkv_store` | រក្សាទុកវដ្តជីវិត និងការកំណត់ផ្លូវ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ចាក់ឡើងវិញហើយបើកម្តងទៀត |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | តារាងពិន្ទុពីចុងដល់ចុង |