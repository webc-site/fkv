# ปุ่มธรรมดาและปุ่มไอคอนกลม

## ชื่อชั้นเรียน

- ปุ่มปกติ`.Btn`
- ปุ่มไอคอน `.BtnC`
- สถานะไฮไลต์หลัก`.Btn.Main`

## ใช้

เพิ่มชื่อคลาสให้กับองค์ประกอบ:

```html
<!-- ปุ่มปกติ -->
<button class="Btn">ยืนยัน</button>

<!-- ไฮไลท์หลัก -->
<button class="Btn Main">ส่ง</button>

<!-- ปุ่มลิงค์ -->
<a class="Btn" href="https://github.com">ปุ่มลิงค์</a>
```

## สไตล์ไอคอน

ไอคอนสำหรับ `BtnC` สามารถปรับแต่งได้ผ่าน `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```