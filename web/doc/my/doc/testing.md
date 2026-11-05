# စမ်းသပ်

## ပြည်တွင်း

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` မျှဝေထားသော ကမ္ဘာလုံးဆိုင်ရာ မှတ်ပုံတင်ခြင်းကို အသုံးပြုပါ။ `cargo test` တိုက်ရိုက်အသုံးပြုသည့်အခါ ထည့်ပါ။
`--test-threads=1`။

## Linux ကွန်တိန်နာများ

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

script သည် Linux test binary ကို cross-compile လုပ်ရန် zigbuild ကိုအသုံးပြုပြီး ၎င်းကို run ရန် အနည်းဆုံး Debian image ထဲသို့ ထည့်သည်။

## CI

`ci.yml` fmt၊ clippy၊ စမ်းသပ်မှုများ အပြည့်အစုံနှင့် RustFS S3 ပေါင်းစည်းမှု စမ်းသပ်မှုများကို လုပ်ဆောင်ပါ။
`linux-test.yml` Ubuntu 24.04 LTS တွင် ညစဉ်ညတိုင်း ပြန်လာခဲ့ပြီး 4 KiB စာမျက်နှာများကို အခိုင်အမာဖော်ပြခဲ့သည်။