# बंद करा बटणासह पॉपअप स्तर तयार करा आणि प्रदर्शित करा

- पॉपअप स्तर तयार करण्यासाठी आणि प्रदर्शित करण्यासाठी कार्यात्मक कॉल
- बिल्ट-इन क्लोज बटण, बंद करण्यासाठी क्लिक करा आणि DOM मधून घटक काढा
- इनपुट बॉक्स डीफोकस करण्यासाठी किंवा पॉपअप लेयर बंद करण्यासाठी Esc दाबा

## डेमो वापरा

```html
<button>पॉप अप करण्यासाठी क्लिक करा</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "मजकूर";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## इंटरफेस पॅरामीटर्स

डीफॉल्ट एक्सपोर्ट फंक्शन `HTMLDialogElement` परत करते आणि `Box` आणि `X` शैली वर्ग डायलॉग घटकामध्ये जोडले गेले आहेत.

## शैली वर्ग

### `.Box.X`

`dialog` घटकावर लागू, संवाद शैली निर्दिष्ट करते.

### `a.X`

`a` घटकावर लागू केलेले, बंद करा बटण शैली निर्दिष्ट करते.