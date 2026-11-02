# Observabilité : étude (à valider)

Version du 2026-11-02. Demande de l'utilisateur : « travailler l'observabilité de l'app », en
partant de l'option B : le Prometheus et le Grafana gérés que Fly offre gratuitement, plutôt que
l'installation `monitoring/` du dépôt. Rien n'est codé : ce document fait l'état des lieux,
décrit ce que je propose, avec des maquettes, et finit par les questions à trancher.

## Le but

Répondre, sans rien payer de plus, à quatre questions :

1. **Est-ce que le bot est utilisé ?** Utilisateurs actifs, photos lues, recettes créées.
2. **Combien coûte Gemini ?** Tokens et dollars par jour, par photo.
3. **Est-ce que ça marche ?** Erreurs, pannes de Gemini, redémarrages, et une alerte Telegram
   quand quelque chose ne va pas.
4. **Est-ce que c'est rapide ?** Temps de lecture d'une photo, d'affichage d'une fiche.

## État des lieux (2026-11-02)

**Ce qui marche :**
- le bot calcule ses métriques (bibliothèque `metrics`) et les publie sur `/metrics`, en public,
  protégées par un jeton (`METRICS_AUTH_TOKEN`) ;
- `/health/live` et `/health/ready`, la vérification toutes les 10 minutes et l'alerte Telegram
  de « Production Uptime », la notification Telegram de chaque mise en production ;
- les logs, avec `fly logs` (gardés peu de temps, sans recherche).

**Ce qui ne sert à rien :**
- les apps Fly `just-ingredients-monitoring` (Prometheus) et `just-ingredients-grafana`, arrêtées :
  Prometheus n'envoie pas le jeton (chaque lecture reçoit un 401), ne tourne que si quelqu'un
  ouvre sa page, et perd son historique à chaque redémarrage (pas de volume) ;
- leur tableau de bord : en marche ou pas, CPU, threads, durée de fonctionnement ; rien sur
  l'usage ni sur Gemini ;
- 15 des 29 fonctions qui enregistrent des métriques ne sont jamais appelées, dont des
  métriques décoratives (`ai_velocity_gain`, `classical_velocity_baseline`, `bugs_fixed_total`,
  `cargo_build_time_seconds`, `deployments_total`), la rétention, les files d'attente, les KPI ;
- `daily_active_users` est trompeur : il compte des actions, pas des personnes ;
- OpenTelemetry (traces) : le code existe mais ne s'active jamais (aucune adresse configurée) ;
- `ENVIRONMENT`, `LOG_FORMAT` et les autres variables d'observabilité ne sont pas lues
  (IMP-051) : les logs sont toujours en texte, jamais en JSON.

**Ce qui marche et qu'on garde :** les métriques de Gemini (`llm_requests_total` par résultat,
`llm_request_duration_seconds`, `llm_retries_total`, `llm_tokens_total`,
`llm_estimated_cost_micro_usd_total`), les recettes créées, les messages Telegram, les durées
des requêtes en base, le cache des extractions.

## L'option B : les métriques gérées par Fly

Ce que dit la documentation de Fly (consultée le 2026-11-02) :

- Fly collecte **gratuitement** (« no additional charge », avec un préavis s'il le change) les
  métriques de chaque app : le proxy (requêtes, temps de réponse), chaque machine (CPU,
  mémoire, disque, réseau, code de sortie, manque de mémoire), les volumes, et Postgres
  (`pg_*` : taille, connexions, requêtes).
- Il collecte aussi **les nôtres** : une section `[metrics]` dans `fly.toml` (port et chemin) ;
  il les lit toutes les 15 secondes par son réseau interne, en Prometheus.
- Il les garde **environ 15 jours** et les affiche dans un **Grafana géré**,
  [fly-metrics.net](https://fly-metrics.net), avec ton compte Fly ; on peut y créer nos tableaux
  de bord.
- **Pas d'alertes** : Fly n'en fait pas sur les métriques.
- Une API Prometheus (`https://api.fly.io/prometheus/<org>/`, jeton en lecture seule) permet de
  les interroger.

Donc : aucune machine en plus, mais deux trous à combler : **les alertes** et **l'historique
au-delà de 15 jours**.

## Ce que je propose

### 1. Les métriques passent par le réseau interne de Fly

Le bot publie ses métriques sur un **port interne** (9091), sans jeton : ce port n'est pas
déclaré comme service public, donc Fly seul (et nos autres apps) peut le lire ; Internet ne le
voit pas. La section `[metrics]` de `fly.toml` (production et staging) le déclare.

Le `/metrics` public et son jeton disparaissent : une porte d'entrée et un secret de moins.
`/health/live` et `/health/ready` restent publics (Fly et « Production Uptime » s'en servent).

### 2. Des métriques utiles, et seulement elles

On supprime les métriques mortes ou trompeuses, et on garde ou ajoute celles qui répondent aux
quatre questions. Toutes commencent par `ji_` pour les retrouver dans Grafana. **Aucune ne porte
d'identifiant d'utilisateur ni de contenu de recette** : seulement des totaux (vie privée, et
Fly écarte les métriques à trop de valeurs différentes).

| Question | Métrique | Ce qu'elle compte |
|---|---|---|
| Usage | `ji_photos_total{result}` | photos reçues : `read` (lue), `not_recipe`, `unreadable`, `too_large`, `daily_limit`, `gemini_error` |
| Usage | `ji_recipes_created_total{kind}` | recettes créées : `photo`, `linked` (partie d'une photo), et le nombre de parties |
| Usage | `ji_actions_total{action}` | fiche ouverte, renommage, modification, suppression, recherche, `/unites`, lien suivi |
| Usage | `ji_users{period}` | utilisateurs actifs sur 24 h et 7 jours, et inscrits au total (calculé chaque heure depuis la base) |
| Usage | `ji_recipes` | recettes enregistrées au total (idem) |
| Coût | `ji_gemini_requests_total{result}` | appels à Gemini : succès, et chaque type d'erreur |
| Coût | `ji_gemini_tokens_total{kind}`, `ji_gemini_cost_micro_usd_total` | tokens en entrée et en sortie, coût estimé |
| Coût | `ji_gemini_cache_hits_total` | photos déjà lues (pas de nouvel appel payant) |
| Santé | `ji_errors_total{component, kind}` | erreurs : Gemini, Telegram, base de données |
| Santé | `ji_gemini_breaker_open` | 1 quand le disjoncteur de Gemini est ouvert (trop d'échecs de suite) |
| Santé | `ji_started_at_seconds` | heure du dernier démarrage (les redémarrages se voient) |
| Rapidité | `ji_gemini_duration_seconds` | durée d'une lecture de photo, nouvelles tentatives comprises |
| Rapidité | `ji_update_duration_seconds{kind}` | traitement d'un message ou d'un bouton |
| Rapidité | `ji_card_shown_seconds` | affichage d'une fiche (ce que logue « Recipe card shown ») |
| Rapidité | `ji_db_duration_seconds{operation}` | requêtes en base |

Les utilisateurs actifs demandent de savoir quand chacun est venu pour la dernière fois : une
colonne `users.last_seen_at`, mise à jour au plus une fois par heure par utilisateur
(migration 7).

S'y ajoutent, sans rien coder, les métriques que Fly fournit : CPU, mémoire, redémarrages et
manque de mémoire des machines, taille et connexions de Postgres, réponses du proxy.

### 3. Trois tableaux de bord dans le Grafana de Fly

Le JSON des tableaux est gardé dans le dépôt (`monitoring/fly-grafana/`) et importé dans
fly-metrics.net (une fois, à la main, ou à chaque changement).

```
┌─ JustIngredients · Usage ─────────────────────────────────────────────────┐
│ Utilisateurs actifs 24 h  [ 3 ]   7 j [ 5 ]   inscrits [ 7 ]   recettes [ 64 ] │
│ Photos par jour (lues / pas une recette / illisibles / limite)  ▁▃▅▂▇▃▁   │
│ Recettes créées par jour (photo / liée)                          ▁▂▄▁▅▂▁   │
│ Actions par jour (fiche, recherche, renommage, modif., suppr.)   ▂▅▃▆▄▂▁   │
└───────────────────────────────────────────────────────────────────────────┘
┌─ JustIngredients · Gemini et coûts ───────────────────────────────────────┐
│ Coût estimé aujourd'hui [ 0,04 $ ]   7 jours [ 0,21 $ ]   par photo [ 0,003 $ ] │
│ Appels par jour (succès / erreurs par type)                      ▁▃▅▂▇▃▁   │
│ Tokens par jour (entrée / sortie)                                ▁▃▅▂▇▃▁   │
│ Durée d'une lecture : médiane [ 3,8 s ]  95 % [ 7,2 s ]          ───╱╲──   │
│ Photos servies par le cache [ 12 % ]                                       │
└───────────────────────────────────────────────────────────────────────────┘
┌─ JustIngredients · Santé ─────────────────────────────────────────────────┐
│ En marche [ ✔ ]  Disjoncteur Gemini [ fermé ]  Dernier démarrage [ 2 nov. 11:05 ] │
│ Erreurs par composant (Gemini / Telegram / base)                 ▁▁▃▁▁▁▁   │
│ Traitement d'un message : médiane / 95 %   Affichage d'une fiche : 95 %    │
│ CPU, mémoire (machines du bot et de la base)   Taille de la base [ 12 Mo ]  │
└───────────────────────────────────────────────────────────────────────────┘
```

### 4. Les alertes envoyées par le bot lui-même

Fly n'en fait pas, et une alerte interne évite une machine ou un service de plus : le bot
connaît lui-même ses échecs. Il t'envoie un message Telegram (ton chat, comme les
notifications de déploiement ; un secret `OWNER_CHAT_ID` côté Fly), au plus une fois par heure
et par sujet :

| Alerte | Quand |
|---|---|
| 🔴 Gemini en panne | le disjoncteur s'ouvre (plusieurs échecs de suite), puis 🟢 quand il se referme |
| 🟠 Gemini échoue souvent | plus de 30 % d'échecs sur au moins 5 appels dans l'heure |
| 💸 Coût du jour | le coût estimé de la journée dépasse un seuil (1 $ proposé) |
| 🟠 Base de données | une requête échoue alors que la base répondait (une fois par panne) |
| 🔁 Redémarrage | le bot a redémarré : au démarrage, il le dit (sauf après une mise en production, déjà notifiée) |

« Production Uptime » reste l'alerte quand le bot ne répond plus du tout : un bot arrêté ne peut
pas prévenir.

### 5. L'historique au-delà de 15 jours : une table de bilan par jour

Fly garde 15 jours. Pour suivre l'usage sur des mois, le bot tient une petite table
`usage_days` (une ligne par jour : photos lues, recettes créées, utilisateurs actifs, appels et
coût Gemini), mise à jour au fil de la journée (migration 7, avec `last_seen_at`). Elle sert à :

- un **bilan hebdomadaire** envoyé le lundi à 9 h (heure de Paris) sur ton Telegram :

```
📊 Semaine du 26 oct. au 1er nov.
👥 5 utilisateurs actifs (dont 1 nouveau) · 7 inscrits
📸 23 photos lues (2 illisibles) · 🍲 19 recettes créées
💸 Gemini : 0,21 $ (0,009 $ par photo) · 0 panne
```

- une commande **`/stats`**, réservée à toi, qui donne le même bilan à la demande (semaine,
  mois, depuis le début).

### 6. Les logs

Ils restent chez Fly (`fly logs`, et la page « Monitoring » de l'app). On corrige IMP-051 : les
logs passent en JSON en production (`LOG_FORMAT` lu), ce qui permet de les filtrer et, plus
tard si besoin, de les envoyer à un service gratuit (Grafana Cloud). Pas de nouvelle machine
pour les expédier dans cette phase.

### 7. OpenTelemetry

À l'échelle du bot, les durées par étape (histogrammes ci-dessus) et les logs suffisent : je
propose de **retirer OpenTelemetry** (trois bibliothèques, un code jamais actif, des mises à jour
Dependabot qui échouent).

### 8. Le ménage

- Supprimer les apps Fly `just-ingredients-monitoring` et `just-ingredients-grafana`
  (`fly apps destroy`, à lancer par toi : c'est définitif) et le dossier `monitoring/` actuel.
- Supprimer les métriques mortes et leurs fonctions, la mesure des sessions et de la rétention
  jamais branchée, le secret `METRICS_AUTH_TOKEN` (après le passage au port interne).
- Mettre à jour `specs/deployment.md`, le README et `.env.example`.

## Coût

Zéro : les métriques et le Grafana de Fly sont gratuits, les alertes et le bilan sont envoyés
par le bot, et les deux apps de monitoring supprimées ne coûtent plus leur disque.

## Découpage proposé (phase O)

| Tâche | Contenu |
|---|---|
| O1 | Port interne 9091 et `[metrics]` dans `fly.toml` ; `/metrics` public et son jeton retirés |
| O2 | Le catalogue de métriques `ji_*` (ajouts, renommages), les métriques mortes supprimées |
| O3 | Migration 7 (`users.last_seen_at`, `usage_days`) et les gauges `ji_users`, `ji_recipes` |
| O4 | Les alertes Telegram du bot (`OWNER_CHAT_ID`) |
| O5 | Le bilan hebdomadaire et `/stats` |
| O6 | Les trois tableaux de bord (JSON dans le dépôt, import dans fly-metrics.net) |
| O7 | Logs JSON (IMP-051), OpenTelemetry retiré, le ménage (apps, dossier, secrets, docs) |
| RO | Relecture indépendante, puis mise en production |

## Questions

1. **Les alertes** du tableau (Gemini en panne, échecs fréquents, coût du jour, base, redémarrage)
   te vont ? Quel **seuil de coût par jour** (1 $ proposé) ?
2. Le **bilan hebdomadaire** le lundi à 9 h, et la commande **`/stats`** réservée à toi : oui ?
3. Enregistrer la **dernière visite** de chaque utilisateur (`last_seen_at`, sans autre donnée)
   pour compter les actifs : d'accord ?
4. **Retirer OpenTelemetry** : d'accord ?
5. **Supprimer les deux apps de monitoring** chez Fly et le dossier `monitoring/` : d'accord ?
   (Tu lances toi-même les deux `fly apps destroy`.)
6. Remplacer le `/metrics` public par le **port interne** (plus de jeton) : d'accord ?
