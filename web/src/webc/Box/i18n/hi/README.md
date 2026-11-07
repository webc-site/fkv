# पॉपअप परत बनाएं और प्रदर्शित करें

- पॉपअप परत बनाने और प्रदर्शित करने के लिए कार्यात्मक कॉल
- बंद होने पर DOM से तत्वों को स्वचालित रूप से हटा दें
- Esc कुंजी रद्दीकरण व्यवहार को रोकें

## डेमो का प्रयोग करें

```html
<button>पॉप अप करने के लिए क्लिक करें</button>

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
  विवरण.textContent = "पाठ";
  button.className = "Btn Main";
  बटन.टेक्स्टकंटेंट = "ठीक";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## इंटरफ़ेस पैरामीटर

### डिफ़ॉल्ट निर्यातित फ़ंक्शन

`Box()`

- **वापसी मूल्य**: `HTMLDialogElement`, `Box` शैली वर्ग का संवाद तत्व जोड़ा गया है।

## शैली वर्ग

### `.Box`

पोजिशनिंग, सेंटरिंग और बैकग्राउंड मास्क ब्लर सेट करने के लिए `dialog` एलिमेंट पर लागू होता है।

### `.Lg`

एक शैली वर्ग आंतरिक सामग्री पर लागू होता है, जो पृष्ठभूमि रंग, गोल कोनों, छाया और लंबवत लेआउट को सेट करता है, और आंतरिक `h3` और `p` के लेआउट को परिभाषित करता है।