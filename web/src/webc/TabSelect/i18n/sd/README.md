# ٽئب صفحو ڊراپ-ڊائون چونڊ سان

- ليبل ۾ adaptive width drop-down Selection Box (`<c-select>`) شامل ڪرڻ کي سپورٽ ڪري ٿو، جيڪو صرف تڏهن ڏيکاريو ويندو جڏهن موجوده ٽيب صفحو چالو ٿيندو.
- ساخت جي وضاحت: ٻار جو عنصر `nav > a` استعمال ڪري ٿو ليبل نيويگيشن طور، ۽ سڌو ٻار عنصر `b` ڪم ڪري ٿو مواد جي علائقي طور.
- پاڻمرادو ڳنڍيندو آهي `a[value]` ٽيگ، `a` سان `c-select` (گروپ جي قدر جي خاصيت ۽ چونڊيل آپشن جي قيمت جي بنياد تي) ۽ `b[slot]` مواد نوڊ.
- خودڪار طريقي سان جاري رکڻ جي حمايت ڪري ٿو موجوده چالو ٿيل شيء کي `localStorage` ذريعي `key` وصف.
- هر گروهه لاءِ `localStorage` لاءِ `key + '/' + tab` استعمال ڪندي تاريخي ذيلي چونڊ جي خودڪار بچت ۽ بحالي جي حمايت ڪري ٿي.
- `value` وصف ذريعي فعال ٽيب جي وضاحت ڪرڻ جي حمايت ڪري ٿو.

## ڊيمو استعمال ڪريو

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>ٻولي</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>سسٽم</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript هڪ اسڪرپٽنگ ٻولي آهي</b>
    <b slot="rs">Rust هڪ سسٽم-سطح جي پروگرامنگ ٻولي آهي</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS هڪ يونڪس تي ٻڌل گرافڪ آپريٽنگ سسٽم آهي</b>
    <b slot="win">ونڊوز هڪ آپريٽنگ سسٽم آهي جيڪو Microsoft پاران تيار ڪيو ويو آهي</b>
  </b>
</c-tab-select>
```

## ملڪيت

- `key`: مکيه نالو استعمال ڪيو ويو مقامي اسٽوريج جي استحڪام لاءِ
- `value`: في الحال چونڊيل قدر

## حسب ضرورت واقعا

- `change`: ٽئب کي تبديل ڪرڻ وقت شروع ڪيو ويو، `e.value` آهي `value` قدر هن وقت چالو ٿيل اختيار جو.