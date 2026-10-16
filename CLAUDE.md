# CLAUDE.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Point d'entree pour reprendre le projet sans contexte. Version du 2026-10-15.
Regles communes a tous mes projets : `../../Projets/00_moi/CONVENTIONS.md`, `AUTONOMIE.md`.
Qui je suis : `../../Projets/00_moi/IDENTITE.md`.

## Ce qu'est ce projet

Nettoyeur de disque pour le developpeur Linux qui fait de l'IA : treemap par categorie
(modeles, caches, Docker, assistants IA, projets dormants), recommandations classees
`safe` / `review` / `never`, et un script shell genere que l'utilisateur lit et lance
lui-meme. L'outil ne supprime jamais rien. 100 % Rust (coeur en crate, CLI, interface
iced). Open source MIT, anglais (francais en option), publie sous
`github.com/blackberrylabs`. Detail dans `BRIEF.md`.

## Etat au 2026-10-15

BRIEF clos (rev. 6 : 100 % Rust / iced). Etape 1 close (70 regles). Etape 2 faite dans la
nuit du 15 au 16 octobre : le coeur compile, 21 tests verts, clippy propre, scanner egal a
`du` a l'octet pres, bout en bout valide sur un home fictif. **Pas encore compile ni lance
sur la vraie machine** : c'est la premiere chose a faire (commandes ci-dessous). Depot git
local cree, premier commit fait ; le second est prepare (fichiers indexes) et attend une
commande du matin (verrous git a retirer, voir JOURNAL). Detail et suite : `PLAN.md`.

## A lire avant de travailler

`BRIEF.md` (cadrage, surtout §5 regles et §6 tableau), `PLAN.md` (avancement), `JOURNAL.md`
(decisions recentes).

## Commandes

Toutes depuis la vraie machine (Rust installe, cf. Eliot). Rien ici ne supprime quoi que
ce soit : `script` ecrit un fichier que l'on lit avant de le lancer.

1. Finir le depot git et compiler (une fois) — retire deux verrous laisses par le shell de
   Cowork qui ne peut pas effacer de fichiers, fait le commit 2, compile en release :

   `cd /home/nushoko/Bureau/Logiciels/Cleanator && rm -f .git/HEAD.lock .git/objects/maintenance.lock && git add -A && git commit -q -F 00_cadrage/commit_2_message.txt && git log --oneline && cargo build --release 2>&1 | tail -2 && ./target/release/cleanator-cli rules | tail -1`

   Attendu : deux commits, `Finished release`, « 10 rules still have a source to verify ».

2. Verifier le scanner contre `du` sur le vrai disque (quelques minutes, lecture seule) :

   `cd /home/nushoko/Bureau/Logiciels/Cleanator && ./target/release/cleanator-cli scan /home/nushoko --json --depth 0 2>/dev/null | python3 -c "import sys,json; print('cleanator', json.load(sys.stdin)['summary']['total_size'])" && echo "du        $(du -sxB1 /home/nushoko | cut -f1)"`

   Attendu : deux nombres egaux (a quelques Ko pres si des fichiers bougent pendant le
   scan). S'ils different de plus de 1 %, c'est un bug du scanner : le noter dans le JOURNAL.

3. Premiere recommandation reelle, sortie gardee dans `00_cadrage/` (hors git) :

   `cd /home/nushoko/Bureau/Logiciels/Cleanator && ./target/release/cleanator-cli recommend /home/nushoko --explain > 00_cadrage/2026-10-16_recommend_reel.txt 2>&1; head -4 00_cadrage/2026-10-16_recommend_reel.txt`

   Attendu : la ligne « scanned … safe … review … in use … never … » ; le detail est dans le
   fichier, a relire ensemble (chaque ligne porte sa regle).

4. Tests et lint apres toute modification : `cd /home/nushoko/Bureau/Logiciels/Cleanator && cargo test && cargo clippy --all-targets`

5. Script (plus tard, quand les recommandations auront ete relues) :
   `cd /home/nushoko/Bureau/Logiciels/Cleanator && ./target/release/cleanator-cli script /home/nushoko -o 00_cadrage/clean.sh` puis lire `00_cadrage/clean.sh` avant toute chose.

Revenir en arriere : `cargo build` ne touche que `target/` ; le depot git local se remet a
un commit avec `git checkout -- .` ; aucune commande ci-dessus ne modifie le disque hors de
`Logiciels/Cleanator`.

## Regles propres au projet

Toutes dans `BRIEF.md` §5 (R1 a R17). Les trois a ne jamais oublier : R1 jamais de
suppression par l'outil ; R4 un magasin gere se nettoie par la commande de son outil ;
R10 bis metadonnees seulement, fichiers d'identification jamais touches.

## Arborescence

- `00_cadrage/` — references (captures), inventaire du disque.
- `BRIEF.md`, `PLAN.md`, `JOURNAL.md`, `CLAUDE.md` — pilotage, en francais.
- `rules/` — les regles de classement (TOML, anglais), `docs/rules.md` leur schema.
- `Cargo.toml` racine, `crates/core` (scanner, regles, sondeurs, classifieur, script),
  `crates/cli` (`cleanator-cli`), `crates/app` (iced, vide jusqu'au spike), `locales/`
  (vide jusqu'a l'etape 3), `README.md` (anglais), `LICENSE` (MIT).
- `00_cadrage/` est hors git (`.gitignore`) : inventaire personnel, captures, sorties.
