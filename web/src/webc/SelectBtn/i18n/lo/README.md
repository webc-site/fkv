# ການປະສົມປະສານຂອງກ່ອງເລືອກແບບເລື່ອນລົງແລະປຸ່ມ

- Splice ກ່ອງເລືອກແລະປຸ່ມອອກຕາມລວງນອນ
- ບັນທຶກສະຖານະທີ່ເລືອກໄວ້ໃສ່ localStorage ໂດຍອັດຕະໂນມັດ
- ຂັດຂວາງການຄລິກປຸ່ມແລະສົ່ງເຫດການທີ່ກໍາຫນົດເອງ

## ໃຊ້ຕົວຢ່າງ

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ທາງເລືອກ 1</option>
    <option value="option2">ທາງເລືອກ 2</option>
  </c-select>
  <button>ແລ່ນ</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ຊັບສິນ

- `key`: ຊື່ກະແຈທີ່ໃຊ້ສຳລັບ localStorage persistence
- `value`: ຄ່າທີ່ເລືອກໃນປັດຈຸບັນ

## ເຫດການ

- `submit`: ສົ່ງເມື່ອປຸ່ມຖືກຄລິກ, `e.value` ປະຕິບັດຄ່າທີ່ເລືອກໃນປັດຈຸບັນ.