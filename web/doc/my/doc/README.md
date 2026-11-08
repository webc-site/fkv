# fkv

fkv သည် သန့်စင်သော Rust embedded KV သိုလှောင်မှုအင်ဂျင်ဖြစ်သည်။

## ဝိသေသ

- shard တစ်ခုစီ၊ ပူးပေါင်းဆောင်ရွက်သော အလုပ်ကွင်းတစ်ခုအတွက် OS thread တစ်ခု။
- COW B+tree၊ နောက်ဆက်တွဲ-သီးသန့် အပိုင်း IO။
- page/index/manifest/overflow/compression ဖော်မတ်။
- local/memory/simulated/S3 object backend
- standalone / cloud / standby ပုံစံတူမုဒ်။
- C ABI မရှိပါ၊ Python SDK မရှိပါ။ တစ်ခုတည်းသော Rust သည် public interface ဖြစ်သည်။

## ဝင်ပေါက်

- [ဗိသုကာပညာ](architecture.md)
- [Crate](crates.md)
- [စမ်းသပ်](testing.md)
- [စွမ်းဆောင်ရည်](performance.md)