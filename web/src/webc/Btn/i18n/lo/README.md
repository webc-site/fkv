# ປຸ່ມປົກກະຕິແລະປຸ່ມຮູບສັນຍາລັກມົນ

## ຊື່ຫ້ອງຮຽນ

- ປຸ່ມປົກກະຕິ`.Btn`
- ປຸ່ມໄອຄອນ `.BtnC`
- ສະຖານະໄຮໄລ້ຫຼັກ`.Btn.Main`

## ໃຊ້

ເພີ່ມຊື່ຫ້ອງຮຽນໃສ່ອົງປະກອບ:

```html
<!-- ປຸ່ມປົກກະຕິ -->
<button class="Btn">ຢືນຢັນ</button>

<!-- ຈຸດ​ເດັ່ນ​ຕົ້ນ​ຕໍ​ -->
<button class="Btn Main">ສົ່ງ</button>

<!-- ປຸ່ມເຊື່ອມຕໍ່ -->
<a class="Btn" href="https://github.com">ປຸ່ມເຊື່ອມຕໍ່</a>
```

## ໄອຄອນຮູບແບບ

ໄອຄອນສຳລັບ `BtnC`ສາມາດປັບແຕ່ງໄດ້ຜ່ານ `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```