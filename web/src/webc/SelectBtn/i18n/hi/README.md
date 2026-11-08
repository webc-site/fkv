# ड्रॉप-डाउन चयन बॉक्स और बटन का संयोजन

- चयन बॉक्स और बटन को क्षैतिज रूप से विभाजित करें
- चयनित स्थिति को स्वचालित रूप से लोकलस्टोरेज में सहेजें
- इंटरसेप्ट बटन क्लिक करें और कस्टम ईवेंट भेजें

## डेमो का प्रयोग करें

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">विकल्प 1</option>
    <option value="option2">विकल्प 2</option>
  </c-select>
  <button>दौड़ना</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## संपत्ति

- `key`: लोकलस्टोरेज दृढ़ता के लिए उपयोग किया जाने वाला कुंजी नाम
- `value`: वर्तमान में चयनित मान

## आयोजन

- `submit`: बटन क्लिक करने पर भेजा जाता है, `e.value` वर्तमान में चयनित मान रखता है