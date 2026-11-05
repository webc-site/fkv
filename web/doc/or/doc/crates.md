# Crate

| ସ୍ତର | Crate | ଦାୟିତ୍ ibilities ଗୁଡିକ | |
|---|---|---|
| foundation | `fkv_types` | ID, pattern ାଞ୍ଚା, ସ୍ଥିର | |
| foundation | `fkv_error` | ତ୍ରୁଟି ମଡେଲ | |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | ବିନ୍ୟାସ ଏବଂ INI ବିଶ୍ଳେଷଣ | |
| foundation | `fkv_metrics` | କାଉଣ୍ଟର ଏବଂ ହିଷ୍ଟୋଗ୍ରାମ୍ | |
| foundation | `fkv_failpoint` | ତ୍ରୁଟି ଇଞ୍ଜେକ୍ସନ୍ | |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | କଞ୍ଚା / ଅଭିଧାନ ସଙ୍କୋଚନ | |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ଆଲାଇନ୍ ବଫର୍ | |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ବଡ଼ ମୂଲ୍ୟ ଏବଂ ବିଭାଗ |
| io | `fkv_io` | ଅସନ୍ତୁଳିତ IO ​​ଗୁଣ | |
| io | `fkv_io_compio` | compio ବ୍ୟାକେଣ୍ଡ୍ | |
| io | `fkv_io_mem` | ମେମୋରୀ ବ୍ୟାକେଣ୍ଡ୍ | |
| io | `fkv_io_sim` | ସିମୁଲେସନ୍ ବ୍ୟାକେଣ୍ଡ୍ | |
| io | `fkv_object` | ବସ୍ତୁ ସଂରକ୍ଷଣ ଗୁଣ | |
| io | `fkv_object_s3` | S3 ବ୍ୟାକେଣ୍ଡ୍ | |
| io | `fkv_standby` | ଷ୍ଟାଣ୍ଡବାଇ କପି | |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ୱାର୍କ ଲୁପ୍ ଏବଂ ଆକ୍ସେସ୍ | |
| engine | `fkv_store` | ଜୀବନଚକ୍ର ଏବଂ ରାଉଟିଙ୍ଗ୍ ଗଚ୍ଛିତ କରନ୍ତୁ | |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ପୁନ play ଚଲାନ୍ତୁ ଏବଂ ପୁନ op ଖୋଲନ୍ତୁ | |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ଶେଷ-ରୁ-ଶେଷ ମାନଦଣ୍ଡ | |