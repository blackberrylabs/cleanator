# CLAUDE.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Point d'entree pour reprendre le projet sans contexte. Version du 2026-10-17.
Regles communes a tous mes projets : `../../Projets/00_moi/CONVENTIONS.md`, `AUTONOMIE.md`.
Qui je suis : `../../Projets/00_moi/IDENTITE.md`.

## Ce qu'est ce projet

Nettoyeur de disque pour le developpeur Linux qui fait de l'IA : treemap par categorie
(modeles, caches, Docker, assistants IA, projets dormants), recommandations classees
`safe` / `review` / `never`, et un script shell genere que l'utilisateur lit et lance
lui-meme. L'outil ne supprime jamais rien. 100 % Rust (coeur en crate, CLI, interface
iced). Open source MIT, anglais (francais en option), publie sous
`github.com/blackberrylabs`. Detail dans `BRIEF.md`.

## Etat au 2026-10-17

**0.2.0 publiee** : module de la tour de controle Blackberrylabs (`cleanator-cli report`,
`cleanator [chemin] --select <regle[::cle]>`), interface sur `bbl-ui` 0.1.0 (anglais par
defaut). Un constat par groupe de regle dans le rapport ; gravite par volume libere
(≥ 1 Gio = a regarder, moins = bon a savoir, `never` = information) — `severity_of` dans
`crates/core/src/report.rs`, ecrit dans le CHANGELOG. La tour : `Logiciels/Blackberrylabs`,
marche a suivre d'un module dans son `docs/new-module.md`. Publier le module avant la tour
quand le contrat bouge.

## Etat au 2026-10-16 (soir)

**0.1.0 publiee.** Les cinq etapes du PLAN sont closes : BRIEF (rev. 6), 70 regles toutes
sourcees (`observed` pour deux regles Claude Desktop), coeur Rust (scanner = `du` a l'octet
pres, sondeurs, classifieur, script), CLI, application iced (treemap, trois onglets,
chargement, en/fr, clair/sombre), paquets `.deb`/`.rpm`/AppImage, depot public
`github.com/blackberrylabs/cleanator`, release `v0.1.0`, README avec captures,
CONTRIBUTING, CHANGELOG. Adresse de commit : noreply GitHub d'Alexandre. Le push et toute
release restent soumis a son accord explicite. La suite (0.1.1) est listee dans
`JOURNAL.md`, entree « etape 5 close » ; les idees de modules dans `00_moi/IDEES.md`.

Methode de travail : Claude compile et teste dans le cloud, copie les fichiers sur le PC
(sommes verifiees), prepare `00_cadrage/commit_N_message.txt` ; Alexandre lance la
commande complete (verrous git a retirer : `find .git -name '*.lock' -delete`).
`.gitignore` s'edite sur place, jamais recopie depuis le cloud.

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

5. Application de bureau (spike iced) : `cd /home/nushoko/Bureau/Logiciels/Cleanator && cargo build --release 2>&1 | tail -1 && ./target/release/cleanator`
   (la premiere compilation d'iced prend plusieurs minutes ; `CLEANATOR_THEME=dark` pour
   demarrer en sombre ; un chemin en argument pour analyser autre chose que le home).

6. Script (plus tard, quand les recommandations auront ete relues) :
   `cd /home/nushoko/Bureau/Logiciels/Cleanator && ./target/release/cleanator-cli script /home/nushoko -o 00_cadrage/clean.sh` puis lire `00_cadrage/clean.sh` avant toute chose.

7. Paquets (une fois `cargo install cargo-deb cargo-generate-rpm` fait) :
   `cd /home/nushoko/Bureau/Logiciels/Cleanator && ./packaging/build.sh` → `dist/` (`.deb`, `.rpm`, AppImage, `SHA256SUMS`). Installer le `.deb` demande sudo et l'accord d'Alexandre : `sudo apt install ./dist/cleanator_0.1.0-1_amd64.deb`.

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
  `crates/cli` (`cleanator-cli`), `crates/app` (`cleanator`, iced 0.13.1), `locales/` (`en.json`,
  `fr.json`), `assets/` (icone SVG/PNG, `.desktop`), `README.md` (anglais), `LICENSE` (MIT).
- `00_cadrage/` est hors git (`.gitignore`) : inventaire personnel, captures, sorties.
  Le `.gitignore` s'edite sur place, jamais recopie depuis le cloud (incident du 2026-10-16).
- `packaging/build.sh` → `dist/` (hors git) ; `packaging/.tools/` (appimagetool, hors git).
