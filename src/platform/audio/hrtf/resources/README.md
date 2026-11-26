# HRTF 자료 (Composite.flac)

데스크탑 믹서 PannerNode 의 panningModel "HRTF" 가 쓰는 머리 전달 함수 응답이다. 크롬이 싣는 파일을 바꾸지 않고 그대로 실었다.

- 원본: 크로미엄 `third_party/blink/renderer/platform/audio/resources/Composite.flac` (태그 158.0.8127.88, git 블롭 `209b1747bbc41eb8881346ab8371eda0a46bb1f5`, 136,404 바이트)
- 꼴: FLAC, 16 비트, 2 채널(왼쪽 귀, 오른쪽 귀), 44100 Hz, 61,440 프레임. 응답 240 개를 256 프레임씩 이었다. 방위 0 ~ 345 를 15 도씩 24 개, 방위마다 높이 0, 15, 30, 45, 60, 75, 90, 315, 330, 345 차례다. (WebKit 버그 98294 c9)
- 짓는 법(크로미엄 resources/README): `IRC_Composite_C_R0195_T<방위>_P<높이>.wav` 240 개를 sox 로 잇고 ffmpeg 로 FLAC 으로 줄였다.
- 엔진은 `include_bytes!` 로 데스크탑 실행 파일에 싣고, 크롬처럼 decodeAudioData 와 같은 해석기로 한 번 푼다. (`hrtfdatabase.rs`) 웹(wasm) 빌드에는 들지 않는다. (웹은 브라우저가 같은 자료로 셈한다)

## 출처

- IRCAM 의 LISTEN HRTF 데이터베이스(Olivier Warusfel, IRCAM 무향실 측정)에서 나온 응답이다. 2010 년에 Chris Rogers(Google)가 WebKit 에 넣었고(WebKit r74433, 버그 50998), 크로미엄이 이어 쓴다. 크로미엄과 WebKit 저장소에는 출처나 이용 조건 글이 없다.
- 파이어폭스는 같은 응답을 실으며 이렇게 적는다. (`dom/media/webaudio/blink/IRC_Composite_C_R0195-incl.cpp`)

  > The sample data in the arrays here was derived for Webkit by Chris Rogers
  > through averaging of impulse responses from the IRCAM Listen HRTF Database.
  > The responses here are half the length of the Listen responses.  This
  > sample data has been granted to the Public Domain.

  여러 피험자를 평균했다는 것과 퍼블릭 도메인으로 내놓았다는 것은 모질라 버그 865241 (c17, c19) 에 적힌 것이다.

## IRCAM 고지

LISTEN 자료의 이용 조건은 IRCAM 이 낸 LISTEN SOFA 파일(sofacoustics.org 의 `listen (hrtf)/IRC_1002_R_44100.sofa`)의 License 속성에 있다. 옮겨 실을 때 저작권 고지를 넣으라는 그 조건대로 고지를 그대로 둔다.

```text
Copyright :
Copyright (c) 2014 - 2017 IRCAM. All Rights Reserved
Disclaimer :
IRCAM MAKES NO REPRESENTATION OR WARRANTIES WITH RESPECT TO THE CONTENTS HEREOF AND SPECIFICALLY DISCLAIM ANY IMPLIED WARRANTIES OR MERCHANTABILITY OR FITNESS FOR ANY PARTICULAR PURPOSE. Further, IRCAM reserves the right to revise this database and/or documentation and to make changes from time to time in the content hereof without obligation of IRCAM to notify any person of such revision or change.
Use of Materials :
IRCAM hereby grants users permission to use materials available therein for any purpose-educational, research or commercial. However, each reproduction of any part of the materials must include the copyright notice, if it is present. In addition, as a courtesy, if these materials are used in published research, this use should be acknowledged in the publication. as follows : UMR 9912 - STMS - IRCAM/CNRS/UPMC, 1, place Stravinsky 75004 PARIS FRANCE If these materials are used in the development of commercial products, IRCAM requests that written acknowledgment of such use be sent to:
IRCAM - 1, place Stravinsky 75004 PARIS FRANCE
```

- 상업 제품에 쓰면 IRCAM 이 위 주소로 서면 알림을 보내 달라고 요청한다. ("requests", 조건이 아니라 부탁이다) 엔진을 쓴 게임을 팔 때 보낼지는 그 게임을 내는 쪽이 정한다.
- LISTEN 누리집의 피험자 정보 장은 "COPYRIGHT : Public" 으로 적는다.
