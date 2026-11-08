# 비동기식으로 카드 목록 로드 및 표시

- **비동기 로딩**: 비동기 함수를 통해 데이터 가져오기
- **상태 표시**: 로드, 데이터 비우기, 카드 렌더링의 세 가지 상태를 지원합니다.
- **유연한 레이아웃**: 카드가 행으로 배열됩니다.

## 데모 사용

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["제목 " + i, "설명 " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## 인터페이스 매개변수

### CardLi(loader)

- **로더**: `() => Promise<Array<[string, string, string]>>`, 비동기 데이터 로딩 기능. 각 하위 키의 형식은 `[이름, 소개, 링크]`입니다.

## 스타일 수업

- `.CardLi`: 외부 요소
- `.Card`: 카드 요소
- `.Ing`: 애니메이션 요소 로드 중