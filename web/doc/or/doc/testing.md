# ପରୀକ୍ଷା

## ସ୍ଥାନୀୟ

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` କେସ୍ ଅଂଶୀଦାର ହୋଇଥିବା ଗ୍ଲୋବାଲ୍ ରେଜିଷ୍ଟ୍ରି ବ୍ୟବହାର କରନ୍ତୁ; ସିଧାସଳଖ `cargo test` ବ୍ୟବହାର କରିବା ସମୟରେ ଯୋଗ କରନ୍ତୁ |
`--test-threads=1` |

## ଲିନକ୍ସ ପାତ୍ରଗୁଡିକ |

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

ଲିନକ୍ସ ଟେଷ୍ଟ୍ ବାଇନାରୀକୁ କ୍ରସ୍-କମ୍ପାଇଲ୍ କରିବା ପାଇଁ ସ୍କ୍ରିପ୍ଟ ଜିଗବିଲ୍ଡ ବ୍ୟବହାର କରେ, ଏବଂ ତାପରେ ଏହାକୁ ଚଲାଇବା ପାଇଁ ସର୍ବନିମ୍ନ ଡେବିୟାନ୍ ଇମେଜରେ ରଖେ |

## CI

`ci.yml` fmt, କ୍ଲିପି, ପୂର୍ଣ୍ଣ ପରୀକ୍ଷା ଏବଂ RustFS S3 ଏକୀକରଣ ପରୀକ୍ଷା ଚଲାନ୍ତୁ |
`linux-test.yml` ଉବୁଣ୍ଟୁ 24.04 LTS ରେ ରାତିରେ ଫେରି ଆସି 4 KiB ପୃଷ୍ଠାଗୁଡ଼ିକୁ ନିଶ୍ଚିତ କଲା |