# fkv

fkv ແມ່ນເຄື່ອງຈັກເກັບຮັກສາ KV ທີ່ຝັງຢູ່ໃນ Rust ອັນບໍລິສຸດ.

## ລັກສະນະ

- ຫນຶ່ງ OS ຕໍ່ shard, loop ການເຮັດວຽກຮ່ວມມື.
- COW B+tree, ສ່ວນຕໍ່ທ້າຍ IO ເທົ່ານັ້ນ.
- page/index/manifest/overflow/compression format.
- local/memory/simulated/S3 object backend.
- ໂຫມດ replica standalone / cloud / standby.
- ບໍ່ມີ C ABI, ບໍ່ມີ Python SDK; ພຽງແຕ່ Rust ແມ່ນການໂຕ້ຕອບສາທາລະນະ.

## ທາງເຂົ້າ

- [ສະຖາປັດຕະຍະກໍາ](architecture.md)
- [Crate](crates.md)
- [ການທົດສອບ](testing.md)
- [ການປະຕິບັດ](performance.md)