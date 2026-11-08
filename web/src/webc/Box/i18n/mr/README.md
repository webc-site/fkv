# पॉपअप स्तर तयार करा आणि प्रदर्शित करा

- पॉपअप स्तर तयार करण्यासाठी आणि प्रदर्शित करण्यासाठी कार्यात्मक कॉल
- बंद केल्यावर DOM मधून घटक आपोआप काढून टाका
- Esc की रद्द करण्याच्या वर्तनास प्रतिबंध करा

## डेमो वापरा

```html
<button>पॉप अप करण्यासाठी क्लिक करा</button>

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
  description.textContent = "मजकूर";
  button.className = "Btn Main";
  button.textContent = "ठीक आहे";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## इंटरफेस पॅरामीटर्स

### डीफॉल्ट निर्यात केलेली कार्ये

`Box()`

- **रिटर्न व्हॅल्यू**: `HTMLDialogElement`, `Box` शैली वर्गाचा डायलॉग घटक जोडला गेला आहे.

## शैली वर्ग

### `.Box`

पोझिशनिंग, सेंटरिंग आणि बॅकग्राउंड मास्क ब्लर सेट करण्यासाठी `dialog` घटकावर लागू होते.

### `.Lg`

पार्श्वभूमी रंग, गोलाकार कोपरे, सावली आणि अनुलंब मांडणी सेट करून आणि आतील `h3` आणि `p` लेआउट परिभाषित करून आतील सामग्रीवर शैली वर्ग लागू केला जातो.