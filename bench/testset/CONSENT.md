# Recording kit: consent text (draft)

Up: [README.md](README.md)

Read this to the speaker, or let them read it, before the first recording. Each speaker chooses one of the two levels below. Save the choice as `consent.txt` in their `testset/<speaker>/` folder (one line: `private` or `release`, the date, and the speaker's own initials or pseudonym). Without a `consent.txt` that says `release`, the recordings are treated as `private`.

This text is a draft. It is not legal advice and has not been reviewed by a lawyer; the release level in particular must be reviewed before any recording is shared.

## Magyarul

**Mit kérünk?** Olvass fel hangosan kb. 130 rövid mondatot a `script.tsv` fájlból, egyet-egyet. 20 mondatot másodszor suttogva is felolvasol. Ez kb. 30–60 perc.

**Mi lesz a felvétellel?** A felvételek (`.wav`) és a mondatok szövege a saját Maceden maradnak, a `~/Library/Application Support/Pusmogo/testset/` mappában. A Pusmogo semmit nem tölt fel. A felvételek nem kerülnek a nyilvános kódtárba.

**Két szint, te választasz:**

1. **Csak helyben (`private`).** A felvételeket csak ezen a gépen használjuk arra, hogy megmérjük, mennyire pontosan érti a Pusmogo a beszédet. Senkinek nem adjuk tovább.
2. **Megosztás (`release`).** A fentin túl a felvételeidet a Pusmogo fejlesztői felhasználhatják a beszédfelismerő modell tanítására, és a felvételek egy része nyilvános teszt-adatkészletként megjelenhet. A neved nem jelenik meg, csak egy álnév.

**Visszavonás.** Bármikor meggondolhatod magad: töröld a mappát, vagy szólj, és a még meg nem osztott felvételeket töröljük. Ami már nyilvános adatkészletben vagy egy betanított modellben megjelent, azt utólag nem lehet teljesen visszavonni; ezért a `release` szintet csak akkor válaszd, ha ezzel egyetértesz.

**Mit ne olvass fel?** Csak a forgatókönyv mondatait olvasd. Ne mondj a felvételbe saját adatot (cím, telefonszám, jelszó), és ne vegyél fel mást a beleegyezése nélkül.

## In English

**What we ask.** Read about 130 short lines from `script.tsv` aloud, one at a time. 20 of them are read a second time, whispering. This takes about 30–60 minutes.

**What happens to the recordings.** The recordings (`.wav`) and the line texts stay on your own Mac, in `~/Library/Application Support/Pusmogo/testset/`. Pusmogo uploads nothing. Recordings are never added to the public repository.

**Two levels, your choice:**

1. **Local only (`private`).** The recordings are used only on this Mac, to measure how accurately Pusmogo understands speech. They are not given to anyone.
2. **Release (`release`).** In addition, the Pusmogo developers may use your recordings to train the speech model, and part of them may be published as a public test set. Your name is not published, only a pseudonym.

**Withdrawing.** You can change your mind at any time: delete the folder, or ask, and recordings not yet shared are deleted. Anything already published in a dataset or learned by a trained model cannot be fully withdrawn afterwards, so choose `release` only if you agree with that.

**What not to read.** Read only the lines in the script. Do not say personal data (address, phone number, password) into a recording, and do not record anyone else without their consent.

## Still to decide

- The licence for released recordings (for example CC-BY-4.0 or CC0) and who holds the signed copy.
- Whether two speakers are held out of training as a test-only group (the plan for whispered speech suggests this), and how a speaker learns which group they are in.
