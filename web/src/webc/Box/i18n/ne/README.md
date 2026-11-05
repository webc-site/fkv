# पपअप तह सिर्जना र प्रदर्शन गर्नुहोस्

- पपअप तह सिर्जना गर्न र प्रदर्शन गर्न कार्यात्मक कल
- बन्द हुँदा DOM बाट तत्वहरू स्वतः हटाउनुहोस्
- Esc कुञ्जी रद्द गर्ने व्यवहार रोक्नुहोस्

## डेमो प्रयोग गर्नुहोस्

```html
<button>पप अप गर्न क्लिक गर्नुहोस्</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "शीर्षक";
  description.textContent = "पाठ";
  button.className = "Btn Main";
  button.textContent = "ठीक";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## इन्टरफेस प्यारामिटरहरू

### पूर्वनिर्धारित निर्यात कार्यहरू

`Box()`

- **रिटर्न मान**: `HTMLDialogElement`, `Box` शैली वर्गको संवाद तत्व थपिएको छ।

## शैली वर्ग

### `.Box`

स्थिति, केन्द्रीकरण, र पृष्ठभूमि मास्क ब्लर सेट गर्न `dialog` तत्वमा लागू हुन्छ।

### `.Lg`

पृष्ठभूमि रङ, गोलाकार कुना, छाया र ठाडो लेआउट सेट गर्दै, र भित्री `h3` र `p` को लेआउट परिभाषित गर्दै, भित्री सामग्रीमा लागू गरिएको शैली वर्ग।