# 페이지 가장자리에 프롬프트 팝업 창 표시

- **함수 호출**: 정상 및 오류 프롬프트 인터페이스 제공
- **자동 스택**: 여러 팁이 간격을 자동으로 계산하여 수직으로 쌓습니다.
- **예약 및 수동 종료**: 예약 및 수동 종료 지원

## 데모 사용

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "안내 내용";
});

toastErr((el) => {
  el.textContent = "오류 내용";
});

toast(
  (el) => {
    el.textContent = "99초 후에 자동으로 닫힙니다.";
  },
  99
);
```

## 인터페이스 매개변수

### toast(render, timeout)

프롬프트를 생성하고 표시합니다.

- `render`: `(el) => void`, 렌더링 콜백, `el`은 프롬프트 상자 요소입니다.
- `timeout`: `Number`, 시간 초과(초). 기본값 9. 자동으로 종료되지 않도록 하려면 0으로 설정합니다.
- 반환 값: 프롬프트 상자 요소. `el.close()` 꺼짐을 지원합니다.

### toastErr(render, timeout)

오류 메시지를 생성하고 표시합니다. 매개변수는 `toast`과 동일하며 `.ERR` 스타일 클래스가 있습니다.

## 스타일 수업

- `.Toast`: 프롬프트 상자 기본 클래스.
- `.ERR`: 오류 상태.
- `.animated` / `.fadeInR` / `.fadeOutR`: 애니메이션 전환.
- `.x`: 닫기 버튼.