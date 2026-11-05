# ສ້າງ ແລະສະແດງຊັ້ນຂໍ້ມູນປັອບອັບດ້ວຍປຸ່ມປິດ

- ການໂທທີ່ມີປະໂຫຍດເພື່ອສ້າງ ແລະສະແດງຊັ້ນປັອບອັບ
- ປຸ່ມປິດໃນຕົວ, ຄລິກເພື່ອປິດ ແລະເອົາອົງປະກອບອອກຈາກ DOM
- ກົດ Esc ເພື່ອປັບໂຟກັສກ່ອງປ້ອນຂໍ້ມູນ ຫຼືປິດຊັ້ນຂໍ້ມູນປັອບອັບ

## ໃຊ້ຕົວຢ່າງ

```html
<button>ຄລິກເພື່ອປັອບອັບ</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ຂໍ້ຄວາມ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ຕົວກໍານົດການໃນການໂຕ້ຕອບ

ຟັງຊັນການສົ່ງອອກເລີ່ມຕົ້ນກັບຄືນມາ `HTMLDialogElement`, ແລະ `Box` ແລະ `X` ຫ້ອງຮຽນສະໄຕໄດ້ຖືກເພີ່ມໃສ່ອົງປະກອບຂອງກ່ອງໂຕ້ຕອບ.

## ຫ້ອງຮຽນແບບ

### `.Box.X`

ນຳໃຊ້ກັບອົງປະກອບ `dialog`, ລະບຸຮູບແບບກ່ອງໂຕ້ຕອບ.

### `a.X`

ນຳໃຊ້ກັບອົງປະກອບ `a`, ລະບຸຮູບແບບປຸ່ມປິດ.