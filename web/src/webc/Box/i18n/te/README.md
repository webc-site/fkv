# పాప్అప్ లేయర్‌ని సృష్టించండి మరియు ప్రదర్శించండి

- పాప్అప్ లేయర్‌ని సృష్టించడానికి మరియు ప్రదర్శించడానికి ఫంక్షనల్ కాల్
- మూసివేసినప్పుడు DOM నుండి మూలకాలను స్వయంచాలకంగా తీసివేయండి
- Esc కీ రద్దు ప్రవర్తనను నిరోధించండి

## డెమో ఉపయోగించండి

```html
<button>పాప్ అప్ చేయడానికి క్లిక్ చేయండి</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "శీర్షిక";
  description.textContent = "టెక్స్ట్";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ఇంటర్ఫేస్ పారామితులు

### డిఫాల్ట్ ఎగుమతి ఫంక్షన్లు

`Box()`

- **రిటర్న్ విలువ**: `HTMLDialogElement`, `Box` స్టైల్ క్లాస్ డైలాగ్ ఎలిమెంట్ జోడించబడింది.

## శైలి తరగతి

### `.Box`

పొజిషనింగ్, సెంటరింగ్ మరియు బ్యాక్‌గ్రౌండ్ మాస్క్ బ్లర్‌ని సెట్ చేయడానికి `dialog` ఎలిమెంట్‌కు వర్తిస్తుంది.

### `.Lg`

నేపథ్య రంగు, గుండ్రని మూలలు, నీడ మరియు నిలువు లేఅవుట్‌ను సెట్ చేయడం మరియు అంతర్గత `h3` మరియు `p` యొక్క లేఅవుట్‌ని నిర్వచించడం వంటి అంతర్గత కంటెంట్‌కి ఒక స్టైల్ క్లాస్ వర్తించబడుతుంది.