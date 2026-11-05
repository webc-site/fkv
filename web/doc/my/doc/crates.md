# Crate

| အလွှာ | Crate | တာဝန်များ |
|---|---|---|
| foundation | `fkv_types` | ID၊ ပုံစံ၊ ကိန်းသေ |
| foundation | `fkv_error` | မော်ဒယ်အမှား |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | ဖွဲ့စည်းမှုပုံစံနှင့် INI ခွဲခြမ်းစိတ်ဖြာခြင်း။ |
| foundation | `fkv_metrics` | ကောင်တာများနှင့် Histograms များ |
| foundation | `fkv_failpoint` | အမှားဆေးထိုး |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | အကြမ်း / အဘိဓာန်ချုံ့ |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ကြားခံချိန်ညှိပါ။ |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ကြီးမားသောတန်ဖိုးနှင့်အပိုင်း |
| io | `fkv_io` | Asynchronous IO လက္ခဏာများ |
| io | `fkv_io_compio` | compio နောက်ခံ |
| io | `fkv_io_mem` | Memory နောက်ခံ |
| io | `fkv_io_sim` | သရုပ်သကန်နောက်ခံ |
| io | `fkv_object` | အရာဝတ္ထုသိုလှောင်မှု လက္ခဏာများ |
| io | `fkv_object_s3` | S3 နောက်ခံ |
| io | `fkv_standby` | အသင့်အနေအထား မိတ္တူ |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | work loop နှင့် access |
| engine | `fkv_store` | life cycle နှင့် routing ကို သိမ်းဆည်းပါ။ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ပြန်ဖွင့်ပြီး ပြန်ဖွင့်ပါ။ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | အဆုံးမှ စံနှုန်း |