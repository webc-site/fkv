# પરીક્ષણ

## સ્થાનિક

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` કેસ શેર કરેલ વૈશ્વિક રજિસ્ટ્રીનો ઉપયોગ કરો; `cargo test` સીધો ઉપયોગ કરતી વખતે ઉમેરો
`--test-threads=1`.

## Linux કન્ટેનર

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

સ્ક્રિપ્ટ Linux ટેસ્ટ બાઈનરીને ક્રોસ-કમ્પાઈલ કરવા માટે ઝિગબિલ્ડનો ઉપયોગ કરે છે, અને પછી તેને ચલાવવા માટે ન્યૂનતમ ડેબિયન ઈમેજમાં મૂકે છે.

## CI

`ci.yml` fmt, clippy, સંપૂર્ણ પરીક્ષણો અને RustFS S3 એકીકરણ પરીક્ષણો ચલાવો.
`linux-test.yml` ઉબુન્ટુ 24.04 LTS માં રાત્રે પાછા ફર્યા અને 4 KiB પૃષ્ઠો પર ભાર મૂક્યો.