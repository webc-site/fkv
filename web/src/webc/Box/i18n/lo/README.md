# ສ້າງ ແລະສະແດງຊັ້ນຂໍ້ມູນປັອບອັບ

- ການໂທທີ່ມີປະໂຫຍດເພື່ອສ້າງ ແລະສະແດງຊັ້ນປັອບອັບ
- ເອົາອົງປະກອບອອກຈາກ DOM ໂດຍອັດຕະໂນມັດເມື່ອປິດ
- ປ້ອງກັນພຶດຕິກໍາການຍົກເລີກລະຫັດ Esc

## ໃຊ້ຕົວຢ່າງ

```html
<button>ຄລິກເພື່ອປັອບອັບ</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "ຫົວຂໍ້";
  description.textContent = "ຂໍ້ຄວາມ";
  button.className = "Btn Main";
  button.textContent = "ຕົກລົງ";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ຕົວກໍານົດການໃນການໂຕ້ຕອບ

### ຟັງຊັນທີ່ສົ່ງອອກເລີ່ມຕົ້ນ

`Box()`

- **ຄ່າກັບຄືນ**: `HTMLDialogElement`, ອົງປະກອບກ່ອງໂຕ້ຕອບຂອງ `Box` ຊັ້ນສະໄຕລ໌ໄດ້ຖືກເພີ່ມແລ້ວ.

## ຫ້ອງຮຽນແບບ

### `.Box`

ນຳໃຊ້ກັບອົງປະກອບ `dialog` ເພື່ອຕັ້ງຕຳແໜ່ງ, ການວາງກາງ ແລະ ໜ້າກາກພື້ນຫຼັງມົວ.

### `.Lg`

ຫ້ອງຮຽນສະໄຕລ໌ທີ່ນຳໃຊ້ກັບເນື້ອຫາພາຍໃນ, ກຳນົດສີພື້ນຫຼັງ, ມຸມມົນ, ເງົາ ແລະຮູບແບບແນວຕັ້ງ, ແລະກຳນົດຮູບແບບຂອງພາຍໃນ `h3` ແລະ `p` .