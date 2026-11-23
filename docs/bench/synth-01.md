# Synth 01: baseline on the synthetic bench set (2026-11-07)

First run on the standard synthetic set from `bench/prompts/` (rendered by `scripts/render-bench.py`): 60 Hungarian sentences, 30 Hungarian sentences with English tech/business terms ("mixed"), 20 English sentences, and 6 clips of silence, noise and hum with empty references. Voices: Tünde (Hungarian and mixed), Eddy US (English). **TTS is a proxy, not real speech**; real-speech numbers (FLEURS HU) are still to come. Apple M5 Pro, idle machine, `pusmogo-bench --features whisper`, engine + cleanup, warm engine.

## Findings

1. **Silence hallucinates on every clip (6/6).** Digital silence and a 50 Hz hum give "NAMASTE"; noise at −60 to −35 dBFS gives "Köszönöm.". In the app, releasing the key without speaking would type this text. This is the top accuracy bug: the hallucination guard item.
2. **Mixed Hungarian-English is the weak spot: WER 19.2 %** vs 7.5 % for plain Hungarian. English loanwords are spelled phonetically or split ("screenshotot" → "strejens hotot", "churn" → "húrn", "Rebase-eld a branchedet" → "Rebass-e eldabran helyetet"). Target for vocabulary hints (D1).
3. **Plain Hungarian errors are mostly double consonants** ("áttettük" → "átetük", "vette" → "vete", "mellékelt" → "melékelt"). Part of this is likely the TTS voice, so real speech must confirm it before tuning for it.
4. **Some counted errors are normaliser gaps, not recognition errors:** "tizenöt százalékkal" vs "15%-al", "nyolckor" vs "8-kor", and compound splits ("marketingkampány" vs "marketing kampány", "negyedévben" vs "negyed évben", "időjárás-előrejelzés" vs "időjárás előrejelzés"). The bench should handle suffixed numbers and score compound splits separately. Note "15%-al" is also wrong Hungarian ("15%-kal"): a cleanup fix for D2.
5. **English is clean: 0 % WER** on 20 sentences.
6. **Latency does not depend on clip length** (2–5 s clips cost the same): about 0.72 s for the first ~15 clips, then about 1.1–1.2 s for the rest of the run (p50 1.14 s, max 2.02 s). The step mid-run points to thermal throttling or background load, so absolute numbers are loose; the flat shape confirms the 30 s encoder window dominates (B3).

Engine `whisper` (`ggml-large-v3-turbo-q5_0.bin`), load 0.20 s, cold first call 0.73 s (excluded below).

| set | clips | audio s | WER % | CER % | CER no-accent % | RTF | p50 s | p95 s |
|---|---|---|---|---|---|---|---|---|
| hu/hu | 60 | 213.4 | 7.5 | 1.6 | 1.6 | 0.310 | 1.14 | 1.31 |
| mixed/hu | 30 | 93.8 | 19.2 | 4.5 | 4.3 | 0.367 | 1.14 | 1.18 |
| en/en | 20 | 69.6 | 0.0 | 0.0 | 0.0 | 0.295 | 1.11 | 1.18 |
| silence/hu | 6 | 20.0 | 100.0 | 100.0 | 100.0 | 0.328 | 1.09 | 1.11 |
| all | 116 | 396.7 | 9.5 | 2.9 | 2.8 | 0.322 | 1.13 | 1.25 |

| id | lang | audio s | latency s | WER % | reference | output |
|---|---|---|---|---|---|---|
| hu001 | hu | 3.5 | 0.72 | 0.0 | Holnap délután háromkor találkozunk az irodában. | Holnap délután háromkor találkozunk az irodában. |
| hu002 | hu | 3.4 | 0.72 | 0.0 | Kérlek, küldd el a jelentést péntekig a csapatnak. | Kérlek, küldd el a jelentést péntekig a csapatnak. |
| hu003 | hu | 4.0 | 0.73 | 0.0 | A szoftverfejlesztés legnehezebb része a nevek kitalálása. | A szoftverfejlesztés legnehezebb része a nevek kitalálása. |
| hu004 | hu | 4.2 | 0.74 | 0.0 | Összesen huszonhét ügyfél jelentkezett az új szolgáltatásra. | Összesen 27 ügyfél jelentkezett az új szolgáltatásra. |
| hu005 | hu | 3.8 | 0.74 | 0.0 | Szerintem érdemes lenne még egyszer átnézni a szerződést. | Szerintem érdemes lenne még egyszer átnézni a szerződést. |
| hu006 | hu | 2.5 | 0.73 | 50.0 | A megbeszélést áttettük jövő hét keddre. | A megbeszélést átetük jövő hétkedre. |
| hu007 | hu | 3.5 | 0.73 | 0.0 | Elnézést a késésért, a vonat húsz percet állt. | Elnézést a késésért, a vonat 20 percet állt. |
| hu008 | hu | 3.8 | 0.74 | 0.0 | Ma este nem érek rá, de szombaton szívesen találkozom. | Ma este nem érek rá, de szombaton szívesen találkozom. |
| hu009 | hu | 2.6 | 0.76 | 80.0 | Légyszi hívj vissza, amikor ráérsz. | Légy si hívej visz, amikor ráérsz. |
| hu010 | hu | 3.6 | 0.96 | 11.1 | A számlát már kiküldtük, a határidő a hónap vége. | A számlát már kikültük, a határidő a hónap vége. |
| hu011 | hu | 3.3 | 2.02 | 0.0 | Köszönöm a gyors választ, így már minden világos. | Köszönöm a gyors választ, így már minden világos. |
| hu012 | hu | 3.6 | 1.47 | 0.0 | Az ügyfél azt kérte, hogy a bemutatót rövidítsük le. | Az ügyfél azt kérte, hogy a bemutatót rövidítsük le. |
| hu013 | hu | 3.9 | 1.14 | 0.0 | Nem találom a tegnapi jegyzőkönyvet, el tudnád küldeni? | Nem találom a tegnapi jegyzőkönyvet, el tudnád küldeni. |
| hu014 | hu | 4.6 | 1.10 | 11.1 | A költségvetés tizenöt százalékkal nőtt az előző évhez képest. | A költségvetés 15%-al nőtt az előző évhez képest. |
| hu015 | hu | 3.5 | 1.11 | 14.3 | A kávéautomata megint elromlott a harmadik emeleten. | A kávéautomata megint eromlott a harmadik emeleten. |
| hu016 | hu | 2.9 | 1.13 | 33.3 | Jövő hónapban indul az új marketingkampány. | Jövő hónapban indul az új marketing kampány. |
| hu017 | hu | 3.7 | 1.18 | 0.0 | A kollégák többsége inkább otthonról dolgozik pénteken. | A kollégák többsége inkább otthonról dolgozik pénteken. |
| hu018 | hu | 4.0 | 1.23 | 0.0 | Kérlek, nézd át a prezentációt, mielőtt elküldöm. | Kérlek, nézd át a prezentációt, mielőtt elküldöm. |
| hu019 | hu | 3.5 | 1.19 | 33.3 | Az időjárás-előrejelzés szerint holnap esni fog. | Az időjárás előrejelzés szerint holnap esni fog. |
| hu020 | hu | 3.7 | 1.13 | 0.0 | Vegyél tejet, kenyeret és egy kis sajtot hazafelé. | Vegyél tejet, kenyeret és egy kis sajtot hazafelé. |
| hu021 | hu | 3.4 | 1.10 | 0.0 | A gyerekeket négy órakor kell elhozni az iskolából. | A gyerekeket négy órakor kell elhozni az iskolából. |
| hu022 | hu | 4.0 | 1.13 | 0.0 | A vonat Budapestről Debrecenbe két és fél óra alatt ér oda. | A vonat Budapestről Debrecenbe két és fél óra alatt ér oda. |
| hu023 | hu | 4.7 | 1.35 | 11.1 | Az új munkatárs hétfőn kezd, kérlek, segíts neki beilleszkedni. | Az új munkatárs hétfőn kezd, kérlek, segíts neki beéleszkedni. |
| hu024 | hu | 3.7 | 1.31 | 0.0 | Ezt a feladatot sajnos nem tudom határidőre befejezni. | Ezt a feladatot sajnos nem tudom határidőre befejezni. |
| hu025 | hu | 4.2 | 1.21 | 10.0 | Már háromszor próbáltam elérni, de nem vette fel a telefont. | Már háromszor próbáltam elérni, de nem vete fel a telefont. |
| hu026 | hu | 3.2 | 1.10 | 0.0 | A projekt első fázisa sikeresen lezárult. | A projekt első fázisa sikeresen lezárult. |
| hu027 | hu | 3.0 | 1.10 | 0.0 | Megkaptam a csomagot, minden rendben volt benne. | Megkaptam a csomagot, minden rendben volt benne. |
| hu028 | hu | 3.5 | 1.12 | 0.0 | Javaslom, hogy a döntést halasszuk el a jövő hétre. | Javaslom, hogy a döntést halasszuk el a jövő hétre. |
| hu029 | hu | 2.9 | 1.08 | 0.0 | Az árajánlatot holnap reggelig várjuk vissza. | Az árajánlatot holnap reggelig várjuk vissza. |
| hu030 | hu | 3.4 | 1.10 | 20.0 | Szeretnék időpontot foglalni csütörtök délelőttre. | Szeretnék időpontot foglalni csütörtök délelőtre. |
| hu031 | hu | 3.3 | 1.13 | 0.0 | A tárgyaló foglalt, menjünk inkább a kisterembe. | A tárgyaló foglalt, menjünk inkább a kisterembe. |
| hu032 | hu | 3.5 | 1.26 | 0.0 | Örülök, hogy végül sikerült megoldani a problémát. | Örülök, hogy végül sikerült megoldani a problémát. |
| hu033 | hu | 4.3 | 1.09 | 9.1 | A könyvelő azt mondta, hogy a bevallást március végéig kell beadni. | A könyvelő azt mondta, hogy a bevalást március végéig kell beadni. |
| hu034 | hu | 3.3 | 1.04 | 0.0 | Ne felejtsd el lezárni az ajtót, amikor elmész. | Ne felejtsd el lezárni az ajtót, amikor elmész. |
| hu035 | hu | 3.8 | 0.95 | 10.0 | A felhasználók több mint fele mobilról nyitja meg az oldalt. | A felhasználók több mint fele mobíról nyitja meg az oldalt. |
| hu036 | hu | 3.9 | 0.90 | 28.6 | Holnap szabadságon leszek, sürgős ügyben írj üzenetet. | Holnap szabadságon leszek, sírgős ügyben éri üzenetet. |
| hu037 | hu | 3.6 | 1.01 | 0.0 | A szállítás várhatóan három munkanapot vesz igénybe. | A szállítás várhatóan három munkanapot vesz igénybe. |
| hu038 | hu | 3.8 | 1.18 | 28.6 | Ebben a negyedévben jelentősen javultak az eredményeink. | Ebben a negyed évben jelentősen javultak az eredményeink. |
| hu039 | hu | 3.5 | 1.25 | 0.0 | Kérlek, írd meg, melyik időpont felel meg neked. | Kérlek, írd meg, melyik időpont felel meg neked. |
| hu040 | hu | 3.3 | 1.18 | 0.0 | A konferencia Szegeden lesz, október közepén. | A konferencia Szegeden lesz, október közepén. |
| hu041 | hu | 3.7 | 1.15 | 0.0 | A hibát megtaláltuk, a javítás már folyamatban van. | A hibát megtaláltuk, a javítás már folyamatban van. |
| hu042 | hu | 3.5 | 1.14 | 0.0 | Nagyon tetszett a tegnapi előadásod, gratulálok. | Nagyon tetszett a tegnapi előadásod, gratulálok! |
| hu043 | hu | 3.6 | 1.16 | 0.0 | A szerződés módosítását a jogászunk is átnézte. | A szerződés módosítását a jogászunk is átnézte. |
| hu044 | hu | 3.5 | 1.17 | 37.5 | Az étterem csak hétkor nyit, addig sétáljunk egyet. | Az éterem csak hétkor mét, adig sétáljunk egyet. |
| hu045 | hu | 3.9 | 1.19 | 10.0 | A raktárban már csak tizenkét darab maradt ebből a termékből. | A raktárban már csak tizenkép darab maradt ebből a termékből. |
| hu046 | hu | 3.8 | 1.18 | 14.3 | Kérem, a mellékelt dokumentumot aláírva küldje vissza. | Kérem, a melékelt dokumentumot aláírva küldje vissza. |
| hu047 | hu | 3.6 | 1.16 | 14.3 | Pénteken korábban zárunk, mert céges vacsora lesz. | Pénteken korában zárunk, mert céges vacsora lesz. |
| hu048 | hu | 3.4 | 1.15 | 0.0 | A nyári szünet alatt a csapat fele szabadságon lesz. | A nyári szünet alatt a csapat fele szabadságon lesz. |
| hu049 | hu | 3.1 | 1.14 | 0.0 | Az új irodában sokkal több a természetes fény. | Az új irodában sokkal több a természetes fény. |
| hu050 | hu | 3.9 | 1.16 | 11.1 | Szerinted melyik változat jobb, az első vagy a második? | Szerinted melyik változati jobb, az első vagy a második? |
| hu051 | hu | 3.3 | 1.15 | 0.0 | A gyógyszertár a sarkon túl, a pékség mellett van. | A gyógyszertár a sarkon túl, a pékség mellett van. |
| hu052 | hu | 3.8 | 1.16 | 0.0 | Elfelejtettem a jelszavamat, kérlek, állítsd vissza. | Elfelejtettem a jelszavamat, kérlek, állítsd vissza! |
| hu053 | hu | 2.9 | 1.15 | 20.0 | A határidőt egy héttel meghosszabbították. | A határidőt egy héttel meghosszabították. |
| hu054 | hu | 3.9 | 1.15 | 12.5 | A jelentés szerint a forgalom harminc százalékkal emelkedett. | A jelentés szerint a forgalom 30%-al emelkedett. |
| hu055 | hu | 2.6 | 1.15 | 16.7 | Holnap reggel nyolckor indulunk a reptérre. | Holnap reggel 8-kor indulunk a reptére. |
| hu056 | hu | 2.5 | 1.18 | 14.3 | Ezt még meg kell beszélnem a vezetőmmel. | Ezt még meg kell beszélnem a vezetőmel. |
| hu057 | hu | 4.1 | 1.20 | 0.0 | A felmérés eredményeit a jövő heti értekezleten mutatjuk be. | A felmérés eredményeit a jövő heti értekezleten mutatjuk be. |
| hu058 | hu | 3.4 | 1.15 | 0.0 | Kérlek, foglalj asztalt négy főre szombat estére. | Kérlek, foglalj asztalt négy főre szombat estére. |
| hu059 | hu | 2.7 | 1.13 | 0.0 | A régi rendszert az év végéig kivezetjük. | A régi rendszert az év végéig kivezetjük. |
| hu060 | hu | 4.0 | 1.16 | 0.0 | Minden kérdésre válaszoltam, ami a levelében szerepelt. | Minden kérdésre válaszoltam, ami a levelében szerepelt. |
| mixed001 | hu | 3.7 | 1.15 | 20.0 | Tegnap pusholtam a branchet, de a build még mindig piros. | Tegnap pusholtam a Branhet, de a bujild még mindig piros. |
| mixed002 | hu | 3.8 | 1.14 | 12.5 | Csináltam egy pull requestet, nézd meg, ha ráérsz. | Csináltam egy pull regvestet, nézd meg, ha ráérsz. |
| mixed003 | hu | 2.8 | 1.14 | 14.3 | A deploy után a staging környezet lehalt. | A deploy után a stagink környezet lehalt. |
| mixed004 | hu | 2.3 | 1.14 | 33.3 | Mergeltem a feature branchet a mainbe. | Mergeltem a Feature Branhet a maimbe. |
| mixed005 | hu | 2.4 | 1.15 | 0.0 | Írj egy unit testet erre a függvényre. | Írj egy unit testet erre a függvényre. |
| mixed006 | hu | 3.6 | 1.17 | 0.0 | A backend API lassan válaszol, nézzük meg a logokat. | A Backend Api lassan válaszol, nézzük meg a logokat. |
| mixed007 | hu | 3.8 | 1.18 | 25.0 | Frissítettem a dependency-ket, és most elszállt a CI. | Frissítettem a dependenciket, és most elszállt a CE-i. |
| mixed008 | hu | 2.2 | 1.14 | 14.3 | Küldd át a Figma linket a Slacken. | Küldd át a Figma linket a slaken. |
| mixed009 | hu | 2.7 | 1.14 | 33.3 | A meetinget áttettem a Google Calendarban. | A mehetinget átettem a Google Calendar-ban. |
| mixed010 | hu | 3.5 | 1.16 | 14.3 | A frontend React komponensei még nincsenek tesztelve. | A frontend reacit komponensei még nincsenek tesztelve. |
| mixed011 | hu | 3.0 | 1.13 | 12.5 | Nyitottam egy issue-t a GitHubon erről a bugról. | Nyitottam egy isuet a GitHub-on erről a bugról. |
| mixed012 | hu | 3.0 | 1.14 | 0.0 | A Docker image túl nagy, optimalizálni kéne. | A Docker image túl nagy, optimalizálni kéne. |
| mixed013 | hu | 3.0 | 1.17 | 50.0 | Rebase-eld a branchedet, mert konfliktus van. | Rebass-e eldabran helyetet, mert konfliktus van. |
| mixed014 | hu | 3.6 | 1.16 | 25.0 | A sprint review csütörtökön lesz, a retro pénteken. | A Sprint Revief csütörtökön lesz, a Retro Pinteken. |
| mixed015 | hu | 3.2 | 1.18 | 20.0 | A product owner szerint ez a feature nem fér bele. | A Product-Ovner szerint ez a Feature nem fér bele. |
| mixed016 | hu | 3.2 | 1.15 | 50.0 | Lefuttattam a benchmarkot, a latency javult. | Lefutattam a Benchmarkot, a latencia volt. |
| mixed017 | hu | 3.5 | 1.18 | 25.0 | A database migration még nem futott le productionben. | A database emigration még nem futott le producciómban. |
| mixed018 | hu | 3.3 | 1.15 | 16.7 | Kérlek, review-zd a kódot, mielőtt mergelem. | Kérlek, reviezz a kódot, mielőtt mergelem. |
| mixed019 | hu | 2.7 | 1.12 | 57.1 | A dashboardon látszik, hogy nőtt a churn. | A dasch boardon látszik, hogy nőt a húrn. |
| mixed020 | hu | 3.1 | 1.12 | 33.3 | Beállítottam az onboarding e-mailt a Mailchimpben. | Beállítottam az onboarding email-t a mail hinben. |
| mixed021 | hu | 2.9 | 1.11 | 16.7 | A cache invalidálás nem működik rendesen. | A CAHE invalidálás nem működik rendesen. |
| mixed022 | hu | 3.0 | 1.13 | 80.0 | Csinálj egy screenshotot a hibaüzenetről. | Csinálj egy strejens hotot a híba üzenetről. |
| mixed023 | hu | 3.2 | 1.15 | 14.3 | A cloud költségeink ebben a hónapban megugrottak. | A Cloud költségeink ebben a hónabban megugrottak. |
| mixed024 | hu | 2.9 | 1.15 | 0.0 | Az MVP-t két hét alatt meg tudjuk csinálni. | Az MVP-t két hét alatt meg tudjuk csinálni. |
| mixed025 | hu | 2.8 | 1.14 | 0.0 | Átírtam a promptot, most jobb a válasz. | Átírtam a promptot, most jobb a válasz. |
| mixed026 | hu | 3.6 | 1.18 | 0.0 | A deadline péntek, de a scope még mindig nem fix. | A deadline péntek, de a scope még mindig nem fix. |
| mixed027 | hu | 3.4 | 1.18 | 0.0 | Kicsit refaktoráltam a kódot, most olvashatóbb. | Kicsit refaktoráltam a kódot, most olvashatóbb. |
| mixed028 | hu | 3.6 | 1.14 | 0.0 | Az endpoint négyszáznégy hibát dob, ha nincs token. | Az endpoint 404 hibát dob, ha nincs token. |
| mixed029 | hu | 3.2 | 1.12 | 22.2 | Nézd meg a Jira ticketet, ott van minden részlet. | Nézd meg a jératicketet, ott van minden részlet. |
| mixed030 | hu | 2.8 | 1.12 | 33.3 | Feltöltöttem a release notes-t a Notionbe. | Feltöltöttem a Relea Senotest a Notionbe. |
| en001 | en | 4.0 | 1.11 | 0.0 | Let's schedule the product review for next Tuesday morning. | Let's schedule the product review for next Tuesday morning. |
| en002 | en | 4.1 | 1.11 | 0.0 | The latency dropped by forty percent after the last release. | The latency dropped by 40% after the last release. |
| en003 | en | 3.7 | 1.11 | 0.0 | Please remind me to call the accountant before lunch. | Please remind me to call the accountant before lunch. |
| en004 | en | 3.5 | 1.13 | 0.0 | I pushed the fix to the branch, can you take a look? | I pushed the fix to the branch. Can you take a look? |
| en005 | en | 2.8 | 1.12 | 0.0 | The meeting has been moved to Thursday at three. | The meeting has been moved to Thursday at 3. |
| en006 | en | 3.8 | 1.13 | 0.0 | Thanks for the quick reply, that answers my question. | Thanks for the quick reply that answers my question. |
| en007 | en | 3.8 | 1.19 | 0.0 | Could you send me the slides from yesterday's presentation? | Could you send me the slides from yesterday's presentation? |
| en008 | en | 3.8 | 1.13 | 0.0 | We need to finish the migration before the end of the quarter. | We need to finish the migration before the end of the quarter. |
| en009 | en | 3.8 | 0.97 | 0.0 | I'll be out of office tomorrow, message me if it's urgent. | I'll be out of office tomorrow. Message me if it's urgent. |
| en010 | en | 3.8 | 0.91 | 0.0 | The new onboarding flow reduced support tickets noticeably. | The new onboarding flow reduced support tickets noticeably. |
| en011 | en | 2.8 | 0.87 | 0.0 | Let's grab coffee after the standup. | Let's grab coffee after the stand-up. |
| en012 | en | 3.5 | 0.86 | 0.0 | The invoice was sent on Monday and is due in thirty days. | The invoice was sent on Monday and is due in 30 days. |
| en013 | en | 3.5 | 0.86 | 0.0 | Can you check whether the build passes on the main branch? | Can you check whether the build passes on the main branch? |
| en014 | en | 3.2 | 0.86 | 0.0 | I think we should ship the smaller version first. | I think we should ship the smaller version first. |
| en015 | en | 3.2 | 0.86 | 0.0 | Our users mostly open the app on their phones. | Our users mostly open the app on their phones. |
| en016 | en | 3.3 | 0.86 | 0.0 | Please book a table for four on Saturday evening. | Please book a table for four on Saturday evening. |
| en017 | en | 3.5 | 0.97 | 0.0 | The conference is in Budapest in the middle of October. | The conference is in Budapest in the middle of October. |
| en018 | en | 3.6 | 1.12 | 0.0 | I forgot my password again, could you reset it? | I forgot my password again. Could you reset it? |
| en019 | en | 3.2 | 1.18 | 0.0 | The test suite takes about twelve minutes to run. | The test suite takes about 12 minutes to run. |
| en020 | en | 2.8 | 1.16 | 0.0 | Let's decide on the pricing next week. | Let's decide on the pricing next week. |
| silence-1s | hu | 1.0 | 1.09 | 100.0 |  | NAMASTE |
| silence-5s | hu | 5.0 | 1.08 | 100.0 |  | NAMASTE |
| noise-60db-3s | hu | 3.0 | 1.09 | 100.0 |  | Köszönöm. |
| noise-45db-3s | hu | 3.0 | 1.10 | 100.0 |  | Köszönöm. |
| noise-35db-5s | hu | 5.0 | 1.10 | 100.0 |  | Köszönöm. |
| hum-40db-3s | hu | 3.0 | 1.11 | 100.0 |  | NAMASTE |

## Reproduce

```bash
scripts/render-bench.py
(cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper)
core/target/release/pusmogo-bench --md out.md bench/audio/synth/{hu,mixed,en,silence}.tsv
```
