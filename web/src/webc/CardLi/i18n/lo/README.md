# ໂຫຼດ ແລະສະແດງລາຍການບັດແບບບໍ່ກົງກັນ

- **ການໂຫຼດ Asynchronous**: ເອົາຂໍ້ມູນຜ່ານຟັງຊັນ asynchronous
- ** ການ​ສະ​ແດງ​ສະ​ຖາ​ນະ​ພາບ **​: ສະ​ຫນັບ​ສະ​ຫນູນ​ສາມ​ລັດ​: ການ​ໂຫຼດ​, ຂໍ້​ມູນ​ເປົ່າ​, ແລະ​ການ​ສະ​ແດງ​ບັດ​.
- ** ຮູບແບບທີ່ຍືດຫຍຸ່ນ **: ບັດຖືກຈັດລຽງເປັນແຖວ

## ໃຊ້ຕົວຢ່າງ

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Title" +i, "Description" +i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ຕົວກໍານົດການໃນການໂຕ້ຕອບ

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, ຟັງຊັນການໂຫຼດຂໍ້ມູນ asynchronous. ຮູບແບບຂອງແຕ່ລະກະແຈຍ່ອຍແມ່ນ `[ຊື່, ການແນະນຳ, ລິ້ງ]`.

## ຫ້ອງຮຽນແບບ

- `.CardLi`: ອົງປະກອບນອກ
- `.Card`: ອົງປະກອບບັດ
- `.Ing`: ກຳລັງໂຫຼດອົງປະກອບເຄື່ອນໄຫວ