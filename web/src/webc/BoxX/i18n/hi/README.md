# बंद करें बटन के साथ एक पॉपअप परत बनाएं और प्रदर्शित करें

- पॉपअप परत बनाने और प्रदर्शित करने के लिए कार्यात्मक कॉल
- अंतर्निहित बंद करें बटन, DOM से तत्व को बंद करने और हटाने के लिए क्लिक करें
- इनपुट बॉक्स को डीफोकस करने या पॉपअप परत को बंद करने के लिए Esc दबाएँ

## डेमो का प्रयोग करें

```html
<button>पॉप अप करने के लिए क्लिक करें</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "पाठ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## इंटरफ़ेस पैरामीटर

डिफ़ॉल्ट निर्यात फ़ंक्शन `HTMLDialogElement` लौटाता है, और `Box` और `X` शैली कक्षाएं संवाद तत्व में जोड़ दी गई हैं।

## शैली वर्ग

### `.Box.X`

`dialog` तत्व पर लागू, संवाद शैली निर्दिष्ट करता है।

### `a.X`

`a` तत्व पर लागू, बंद करें बटन शैली निर्दिष्ट करता है।