# ड्रॉप-डाउन निवड बॉक्स आणि बटणाचे संयोजन

- निवड बॉक्स आणि बटण क्षैतिजरित्या विभाजित करा
- निवडलेले राज्य स्वयंचलितपणे लोकल स्टोरेजमध्ये जतन करा
- इंटरसेप्ट बटण क्लिक आणि सानुकूल इव्हेंट पाठवा

## डेमो वापरा

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">पर्याय १</option>
    <option value="option2">पर्याय २</option>
  </c-select>
  <button>धावणे</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## मालमत्ता

- `key`: लोकल स्टोरेज पर्सिस्टन्ससाठी की नाव वापरले
- `value`: सध्या निवडलेले मूल्य

## कार्यक्रम

- `submit`: बटण क्लिक केल्यावर पाठवले जाते, `e.value` सध्या निवडलेले मूल्य असते