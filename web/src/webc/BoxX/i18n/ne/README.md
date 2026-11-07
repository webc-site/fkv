# बन्द बटनको साथ पपअप तह सिर्जना गर्नुहोस् र प्रदर्शन गर्नुहोस्

- पपअप तह सिर्जना गर्न र प्रदर्शन गर्न कार्यात्मक कल
- बिल्ट-इन बन्द बटन, बन्द गर्न क्लिक गर्नुहोस् र DOM बाट तत्व हटाउन
- इनपुट बक्स डिफोकस गर्न वा पपअप तह बन्द गर्न Esc थिच्नुहोस्

## डेमो प्रयोग गर्नुहोस्

```html
<button>पप अप गर्न क्लिक गर्नुहोस्</button>

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


## इन्टरफेस प्यारामिटरहरू

पूर्वनिर्धारित निर्यात प्रकार्यले `HTMLDialogElement` फर्काउँछ, र `Box` र `X` शैली वर्गहरू संवाद तत्वमा थपिएका छन्।

## शैली वर्ग

### `.Box.X`

`dialog` तत्वमा लागू गरियो, संवाद शैली निर्दिष्ट गर्दछ।

### `a.X`

`a` तत्वमा लागू, बन्द बटन शैली निर्दिष्ट गर्दछ।