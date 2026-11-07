# ड्रप-डाउन चयन बाकस र बटनको संयोजन

- चयन बाकस र बटन तेर्सो रूपमा विभाजित गर्नुहोस्
- स्वचालित रूपमा स्थानीय भण्डारणमा चयन गरिएको राज्य बचत गर्नुहोस्
- बटन क्लिकहरू रोक्नुहोस् र अनुकूलन घटनाहरू पठाउनुहोस्

## डेमो प्रयोग गर्नुहोस्

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">विकल्प १</option>
    <option value="option2">विकल्प २</option>
  </c-select>
  <button>दौडनु</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## सम्पत्ति

- `key`: स्थानीय भण्डारण स्थिरताको लागि प्रयोग गरिएको कुञ्जी नाम
- `value`: हाल चयन गरिएको मान

## घटना

- `submit`: बटन क्लिक गर्दा पठाइन्छ, `e.value` हाल चयन गरिएको मान बोक्छ