# ସାଧାରଣ ବଟନ୍ ଏବଂ ରାଉଣ୍ଡ ଆଇକନ୍ ବଟନ୍ |

## ଶ୍ରେଣୀ ନାମ

- ସାଧାରଣ ବଟନ୍ `.Btn` |
- ଆଇକନ୍ ବଟନ୍ `.BtnC` |
- ପ୍ରାଥମିକ ହାଇଲାଇଟ୍ ସ୍ଥିତି `.Btn.Main` |

## ବ୍ୟବହାର କରନ୍ତୁ |

ଉପାଦାନରେ ଏକ ଶ୍ରେଣୀ ନାମ ଯୋଡନ୍ତୁ:

```html
<!-- ସାଧାରଣ ବଟନ୍ | -->
<button class="Btn">ନିଶ୍ଚିତ କର |</button>

<!-- ମୁଖ୍ୟ ହାଇଲାଇଟ୍ | -->
<button class="Btn Main">ଦାଖଲ କର |</button>

<!-- ଲିଙ୍କ୍ ବଟନ୍ | -->
<a class="Btn" href="https://github.com">ଲିଙ୍କ୍ ବଟନ୍ |</a>
```

## ଆଇକନ୍ ଶ style ଳୀ

`BtnC` ପାଇଁ ଆଇକନ୍ `background-image` ମାଧ୍ୟମରେ କଷ୍ଟମାଇଜ୍ ହୋଇପାରିବ:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```