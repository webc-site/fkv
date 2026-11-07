# Crate

| ຊັ້ນ | Crate | ຄວາມຮັບຜິດຊອບ |
|---|---|---|
| foundation | `fkv_types` | ID, ຮູບແບບ, ຄົງທີ່ |
| foundation | `fkv_error` | ຮູບແບບຄວາມຜິດພາດ |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | ການຕັ້ງຄ່າແລະການວິເຄາະ INI |
| foundation | `fkv_metrics` | ເຄົາເຕີ ແລະ ຮິສໂຕແກຣມ |
| foundation | `fkv_failpoint` | ການສັກຢາຜິດ |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / ການບີບອັດວັດຈະນານຸກົມ |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ຈັດວາງ buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ມູນຄ່າຂະຫນາດໃຫຍ່ແລະສ່ວນ |
| io | `fkv_io` | ລັກສະນະ Asynchronous IO |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | ໜ່ວຍຄວາມຈຳດ້ານຫຼັງ |
| io | `fkv_io_sim` | ເບື້ອງຫຼັງການຈຳລອງ |
| io | `fkv_object` | ລັກສະນະການເກັບຮັກສາວັດຖຸ |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | ສຳເນົາສະແຕນບາຍ |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | loop ການເຮັດວຽກແລະການເຂົ້າເຖິງ |
| engine | `fkv_store` | ເກັບຮັກສາວົງຈອນຊີວິດແລະເສັ້ນທາງ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ຫຼິ້ນຄືນ ແລະເປີດຄືນໃໝ່ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ດັດຊະນີຈຸດຈົບ |