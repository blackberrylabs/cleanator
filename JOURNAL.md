# JOURNAL.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Journal antichronologique des decisions. La plus recente en haut. Une entree = une date, un
titre, ce qui a ete decide, par qui, ce qui a ete ecarte, et la preuve (fichier, commit, run).

---

## 2026-10-15, etape 1 close : 70 regles v0

- Alexandre : relecture d'`assistants.toml`, « je ne vois rien de bizarre, pour moi c'est
  bon » ; « tu peux finir l'etape 1 ».
- Fait par Claude : `rules/backups-and-large-files.toml` (5), `rules/tooling.toml` (3),
  `rules/training.toml` (1) ; onze documentations lues de plus (pipx, pytest, vLLM, nvm,
  npx, JetBrains, LM Studio CLI, Aider, gio, tracker3-reset, Gemini CLI) et reportees dans
  les regles ; verification automatique : 70 identifiants uniques, chaque regle avec
  pourquoi / regeneration / cout, aucune incoherence niveau-nettoyage.
- Reste « avant release » (bloquant, pas avant le code) : 10 regles sans documentation
  publique lue (Claude Desktop n'en a pas ; les autres a lire).
- Decision de methode : les regles de Claude Desktop (`vm_bundles` = never, `audit.jsonl`
  = review) reposent sur l'observation de la machine de dev, pas sur une doc ; dit dans
  leurs `notes`.
- Etat : etapes 0 et 1 closes ; etape 2 (coeur Rust) a ouvrir sur le « go » d'Alexandre.

## 2026-10-15, revision 6 : pile 100 % Rust (iced) a la place de Tauri + React

- Decision d'Alexandre : 100 % Rust, interface iced, avec un spike d'une journee a l'etape 3
  (« j'aime pas mal, ca fait pas gadget, ca fait pas non plus trop complique... moi je dis
  oui », apres examen de captures de Sniffnet, Halloy et COSMIC). Argument d'Alexandre :
  la contrainte d'Eliot (editeur graphique) n'existe pas ici. Arguments de Claude, qui a
  change d'avis : interface petite, pas de dependance a webkit2gtk (AppImage Tauri fragiles
  entre distributions), « no webview » pour le public. Ecarte : egui (air d'outil de
  debug), Slint (langage de description a part), Tauri + frontend Rust (petit ecosysteme).
- Couts acceptes : rendu natif propre plutot que clone de la maquette web ; plus
  d'iterations sur l'interface (moins de code iced que de code React dans le monde) ;
  paquets a construire avec cargo-deb / cargo-generate-rpm / appimagetool.
- Garde-fou : spike valide visuellement avant tout branchement ; en cas d'echec, retour a
  Tauri + React sans toucher au coeur. Preuve : BRIEF §6, R14, R16, R17, §10 ; PLAN etapes
  2-4 ; CLAUDE.md ; fiche PROJETS.md.

## 2026-10-15, regles v0 : 61 regles en sept fichiers

- Fait par Claude : `rules/python-envs.toml` (5), `rules/builds.toml` (5),
  `rules/assistants.toml` (14), `rules/system.toml` (13) ; `docs/rules.md` complete
  (`marker`, `requires_sibling`, `exclude_under`, `group_by`, `dormancy_measure`, section
  « ce que l'interface montre »). Les sept fichiers se chargent ; 61 identifiants uniques ;
  16 regles portent une source `to-verify` (bloquant avant release).
- Alexandre : le format (pourquoi + cout) lui suffit pour cocher ; pour un developpeur,
  reponse de Claude : meme donnees, deuxieme profondeur (commande, chemin, date, lien doc,
  id de regle) — exigence notee pour l'etape 3.
- Appris dans les docs : Claude Code balaie lui-meme ses transcriptions apres
  `cleanupPeriodDays` (30 j par defaut) mais garde celles de Desktop/Cowork sans limite
  sauf `desktopSessionCleanupPeriodDays` ; liste explicite des fichiers d'etat a garder.
  Codex : `CODEX_HOME` (`~/.codex`), `history.jsonl`, `persistence = "none"`. snap :
  `refresh.retain=2` sur Ubuntu classique. journald : le vacuum ne touche que les
  journaux archives. flatpak : jamais `--delete-data`.
- Question de conception posee par Alexandre (deux fois) : 100 % Rust plutot que Tauri +
  React ? Voir l'entree suivante une fois tranchee.

## 2026-10-15, etape 1 ouverte : schema des regles et 24 premieres regles

- Fait par Claude : `docs/rules.md` (schema d'une regle, niveaux, kinds de detection
  `path`/`signature`/`pattern`/`probe`, verifications d'usage, tailles, categories,
  confidentialite) ; `rules/package-caches.toml` (10), `rules/docker.toml` (6),
  `rules/models.toml` (8). Chaque regle : pourquoi, regeneration, cout, source lue le
  2026-10-15 avec citation. Les trois fichiers se chargent (TOML valide, champs requis
  presents).
- Appris dans les docs, qui change les regles : uv — « jamais sur de modifier le cache
  directement » → commande seulement, pas de `rm` ; Hugging Face — `hf cache rm` /
  `hf cache prune` remplacent `huggingface-cli delete-cache`, les anciens outils peuvent
  supprimer un blob partage ; conda — `--force-pkgs-dirs` casse les environnements, jamais
  utilise ; Docker — `UNIQUE SIZE` est la seule taille honnete, volumes jamais supprimes
  automatiquement ; Ollama — chemin Linux de l'installeur `/usr/share/ollama/.ollama/models`.
- Deux sources marquees `to-verify` (LM Studio) ou « par convention » (`_npx`) : bloquantes
  avant release, pas avant relecture.
- Etat : en attente de la relecture d'Alexandre, categorie par categorie.

## 2026-10-15, validation finale du BRIEF (rev. 4 et 5)

- Decision d'Alexandre : rev. 4 et 5 validees (« Je valide »). Le BRIEF est clos.
  Preuve : `BRIEF.md` §10.
- Etape 0 du PLAN close ; etape 1 (regles v0) ouverte.

## 2026-10-15, inventaire du vrai disque et revision 5 du BRIEF

- Fait : inventaire en deux passages, lecture seule (`00_cadrage/2026-10-15_inventaire_disque.md`,
  246 + ~200 lignes) ; analyse dans `00_cadrage/2026-10-15_inventaire_analyse.md`.
- Constat : 764 Go utilises ; modeles 131 Go (dont 100 Go hors chemins standard), Claude
  Desktop 67 Go (VM Cowork 20 Go en usage + sauvegarde manuelle 23 Go), Docker 75 + 25 Go
  (49 Go recuperables selon Docker), venvs 48 Go a la racine du home, `target/` Eliot
  26,5 Go, ~60 Go de doublons. Estimation a la main : ~60 Go `safe`, ~180 Go `review`.
- Treize ecarts entre le tableau §6 et la realite, corriges en rev. 5 : detection par
  signature (modeles, venvs, builds, sauvegardes, depots re-clonables) avant detection
  par chemin ; `target/` actif en `review` avec cout ; montages Docker exclus (R6) ;
  Docker = taille unique, images taguees sans conteneur en `review` ; Claude Desktop
  (VM `never`, sessions `review`) ; index tracker3, pytest, caches vLLM en `safe` ;
  sauvegardes et re-clonables en categories ; gros fichiers partout dans le home.
- Alexandre : « c'est pour ca que je cherchais a creer un outil de ce type-la ».
- Etat : rev. 4 et 5 a valider ensemble ; etape 0 du PLAN close a cette validation ;
  ensuite etape 1 (regles v0).

## 2026-10-15, validation du BRIEF (rev. 3) et audit « multitude d'assistants IA » (rev. 4)

- Decision d'Alexandre : BRIEF rev. 3 valide (« Ok, je valide »). Preuve : `BRIEF.md`
  en-tete.
- Audit par Claude, a la demande d'Alexandre (« top pour un developpeur Linux avec
  multitude d'assistants IA ») : vue « assistants IA » ajoutee en §1 (configuration /
  historique / caches, par assistant) ; R6 etendu aux assistants en cours d'execution ;
  R10 bis (metadonnees seulement, fichiers d'identification et de configuration `never`) ;
  tableau §6 : la ligne « Outils IA et editeurs » eclatee en six lignes (historique,
  caches, configuration, serveurs MCP et outils CLI, versions de Node multiples, LLM locaux
  des assistants) ; detection des doublons notee en candidat v2. Rev. 4 a valider.
- `PLAN.md` et `CLAUDE.md` crees depuis les gabarits de `00_moi/modeles/` ; fiche
  Cleanator ajoutee a `00_moi/PROJETS.md` avec `Bureau/Logiciels/` comme domicile des
  logiciels de bureau.
- Etat : en attente de la validation de la rev. 4, puis tache 0 (inventaire du disque).

## 2026-10-15, Q1, Q3, Q5, Q6 : le BRIEF est complet

- Decision d'Alexandre : Q1 — fichiers de pilotage en francais a la racine avec en-tete
  anglais ; documentation publique en anglais seulement. Ecarte : tout en anglais (le BRIEF
  se valide mieux dans la langue de celui qui valide), `docs/fr/` (entorse aux quatre
  fichiers a la racine).
- Decision d'Alexandre : Q3 — 60 jours de dormance, reglable.
- Decision d'Alexandre : Q5 — `Bureau/Logiciels/` est le domicile de tous ses logiciels de
  bureau ; fiche Cleanator a ajouter dans `00_moi/PROJETS.md` a la validation.
- Decision de Claude, sur delegation d'Alexandre (« ce qui est le mieux ») : Q6 —
  `create-tauri-app` propre, puis reprise de ce qu'Eliot a deja resolu.
- Etat : BRIEF rev. 3 complet, aucune question ouverte ; en attente de la validation
  d'Alexandre avant la tache 0 (inventaire du disque).

## 2026-10-15, Q4 et modele economique

- Decision d'Alexandre : Q4 — v1 analyse le dossier personnel + les emplacements systeme
  connus (apt, journald, snap, flatpak, Docker par sa commande) ; disque entier en option.
  Ecarte : disque entier par defaut (root necessaire pour etre juste, scan long).
- Decision d'Alexandre, sur avis de Claude : entierement gratuit, pas de freemium ; Sponsors
  ou edition « equipe » envisageables seulement avec une audience. Ecarte : fonctions
  simples gratuites + fonctions poussees payantes (incompatible avec MIT et la confiance ;
  les fonctions « poussees » sont le coeur qui rend les chiffres justes).
- Principe de produit ajoute au BRIEF §3 : simple, facile, intuitif — une fenetre, scan a
  l'ouverture, trois gestes.
- Etat : quatre questions ouvertes (Q1, Q3, Q5, Q6).

## 2026-10-15, revision 2 du BRIEF : audit « dev IA sous Linux » et Q2

- Decision d'Alexandre : Q2 tranchee — `rm` pour `safe`, corbeille (`gio trash`) pour
  `review` (R13). Ecarte : tout a la corbeille (l'espace ne serait pas libere).
- Audit par Claude, a la demande d'Alexandre (« verifie que l'outil soit bien pense pour les
  developpeurs avec IA, et Linux ») : sept regles ajoutees (R4 magasins geres par la
  commande de leur outil, R5 tailles vraies et liens physiques, R6 jamais ce qui est en
  usage, R7 sorties d'entrainement en `review`, R8 variables d'environnement, R12 blocs
  utilisateur/sudo) ; sondeurs par outil dans l'architecture ; modeles affiches par nom ;
  vue « projets dormants » ; tableau des emplacements passe de 9 a 18 lignes (LM Studio,
  ComfyUI, Whisper, datasets, conda, pnpm/bun, `target/` Rust, Go, Claude Code/Cursor/VS
  Code, CUDA, Timeshift, VM, installeurs). Preuve : `BRIEF.md` rev. 2.
- Etat : BRIEF rev. 2 en attente de validation ; cinq questions ouvertes (Q1, Q3–Q6).

## 2026-10-15, premiere entree : cadrage de la v1

- Decision d'Alexandre : creer Cleanator, nettoyeur de disque pour le developpeur Linux qui
  fait de l'IA ; v1 = Analyser + Nettoyer (treemap + recommandations classees + script
  genere, jamais de suppression par l'outil). Ecarte : Desinstaller, Optimiser (peu de sens
  sous Linux), Statut (remis a plus tard), toute version payante. Preuve : `BRIEF.md`,
  captures `00_cadrage/references/`.
- Decision d'Alexandre : open source MIT, publie sous l'organisation GitHub Blackberrylabs
  (creee le jour meme, 0 depot) ; interface anglais + francais en option ; code, commentaires,
  commits en anglais (derogation R8 a CONVENTIONS.md). Preuve : api.github.com/orgs/blackberrylabs.
- Decision d'Alexandre : pile Tauri + React + Rust, comme Eliot ; installation `.deb` +
  AppImage via `tauri build`. Ecarte : Python + page web locale (deuxieme pile de bureau,
  scan plus lent), Electron.
- Verifie par Claude : nom `cleanator` libre comme compte GitHub (404), quatre depots
  homonymes a 0 etoile sans rapport ; Luno (app.lunomac.com) etudie comme reference.
- Range : les quatre captures d'ecran deplacees de la racine vers `00_cadrage/references/`
  avec des noms dates.
- Etat : BRIEF ecrit, en attente de validation ; six questions ouvertes (BRIEF §10).
