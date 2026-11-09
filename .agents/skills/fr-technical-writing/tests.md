# Tests de sens révisés (FR)

Les exemples sont hypothétiques et ne décrivent aucune fonction Coppice. « Accepté » signifie que le sens est préservé, pas que la phrase est vraie.

## Revérification de T1–T15 (analysis.md)

| Test | Verdict | Correction |
|---|---|---|
| T1 lecture du fichier choisi | OK | — |
| T2 « autorise Léa à consulter ce dossier, sans le modifier » | **Ambigu** | `sans le modifier` peut être une restriction de permission ou une manière de consulter. La réécriture « mais pas à le modifier » tranche à la place de l'auteur. Attendu : signaler l'ambiguïté. |
| T3–T7 | OK | — |
| T8 `pas nécessairement` → `pas systématiquement` | **Faible** | Glissement de la nécessité logique vers la fréquence. Il faut préférer garder `pas nécessairement`. |
| T9 « depuis la version 2 … y compris de sécurité » | OK, plus une réserve | L'ambiguïté « depuis la sortie de la v2 » ou « pour les versions ≥ 2 » est conservée et ne doit pas être tranchée. Les exemples liés à FR29 sont remplacés par FR37/FR27. |
| T10–T12, T14, T15 | OK | — |
| T13 « Vous devez obtenir l'accord » → « Obtenez l'accord » | OK en procédure | Dans une référence, garder `devez` (obligation énoncée, pas une étape). |

## Nouveaux cas adversariaux

| # | Source | Original (type) | Réécriture refusée | Pourquoi |
|---|---|---|---|---|
| T16 | FR37/FR38 (procédé) | « Nous utilisons principalement deux formats. » | « Utilisez deux formats. » | Le constat devient une injonction. Le `nous` communautaire n'est pas une consigne. |
| T17 | FR37 | « À ce jour, 2048 bits est considéré comme raisonnablement sûr. » | « 2048 bits est sûr. » | L'ancrage daté et l'attribution disparaissent, ce qui crée une garantie de sécurité. |
| T18 | FR38 / FR37 | « donne nécessairement false » vs « pas nécessairement » | Échanger les deux | La nécessité devient une non-nécessité. |
| T19 | FR28 | sortie littérale `1,540 octets` | `1 540 octets` | Un littéral serait modifié par la typographie. |
| T20 | FR33 | « ne devraient pas abuser de… » | « ne doivent pas utiliser… » | Une recommandation devient une interdiction, et une restriction de quantité devient totale. |
| T21 | FR10 | « l'emportent, sauf lorsque A, ou que B » | « sauf lorsque A et B » | La disjonction devient une conjonction. |
| T22 | FR35 | « Une raison possible est un parasite. » | « La cause est un parasite. » | Une hypothèse devient un diagnostic. |
| T23 | — | « ne supprime pas que les fichiers temporaires » | « ne supprime pas les fichiers temporaires » | Le sens s'inverse (`ne… pas que` = pas seulement). |
| T24 | FR38 | « `x = x \|\| 10000` donne une valeur par défaut » | « 10000 si x est absent » | 0, "" et false sont aussi remplacés. La condition du défaut est inventée. |
| T25 | FR37 | « vous pouvez choisir librement RSA ou DSA » (2010) | présenter cela comme une permission actuelle | C'était une possibilité historique, pas une recommandation actuelle. |
| T26 | FR10 / FR12 | « devront tenir compte » / « devraient expliciter » | Les fusionner | Obligation et recommandation sont distinctes. |
| T27 | FR18 | « ne PAS utiliser » (contre-exemple) | Le transformer en recommandation | La polarité est perdue. |
| T28 | FR36 | « n'est utile que si… » | « est utile si… » | La restriction (condition nécessaire) devient une condition suffisante. |
| T29 | FR33 | étapes, puis « en dernier ressort » | Réordonner | L'ordre des opérations fait partie du sens. |
| T30 | FR06 | « sera sans doute souvent remplacé » | « sera remplacé » | Une prédiction prudente devient un fait. |
| T31 | FR42 | `"Position déterminée :\n"` dans du code | `« Position déterminée :\n »` ou un ajout d'espace insécable | La chaîne littérale changerait. |
| T32 | — | « Coppice lit le fichier. » | « Coppice lit le fichier en toute sécurité et plus vite. » | Ajoute une garantie et un gain inventés. |
| T33 | — (UI, extrapolation) | bouton « Supprimer » | « Supprimer définitivement » | Invente un effet irréversible non vérifié. |

## Cas ajoutés en passe 2 (sources de remplacement)

| # | Source | Original (type) | Réécriture refusée | Pourquoi |
|---|---|---|---|---|
| T34 | FR39 | « Un git merge ne devrait être utilisé que pour… » | « Utilisez git merge pour… » | Le conditionnel de recommandation et la restriction `ne… que` disparaissent. |
| T35 | FR39 | « Il est alors préférable, voire impératif, que… » | « Il faut que… » ou « Il est conseillé que… » | La gradation (préférence qui peut aller jusqu'à l'obligation) est aplatie. |
| T36 | FR39 | « on ne pushera que 2 à 3 fois, voire moins » | « on pushera 2 à 3 fois » | La borne supérieure et l'ouverture « voire moins » sont perdues. |
| T37 | FR41 | « l'algorithme par défaut est censé être tout à fait adapté » | « l'algorithme par défaut est adapté » | Une attente devient une garantie (sécurité). |
| T38 | FR41 | « (et donc, probablement, PHP 5.5) » | « PHP 5.5 apporte » | Une prédiction d'avant la sortie devient un fait. |
| T39 | FR41 | « veillez à systématiquement utiliser cette fonction, plutôt qu'à re-hasher… » | « vous pouvez utiliser cette fonction » | Une consigne de sécurité devient une simple option. |
| T40 | FR42 | commentaire « Position par défaut (Châtelet à Paris) » + coordonnées 48.5794, 7.7519 | Corriger silencieusement l'un ou l'autre | Signaler l'incohérence et ne pas deviner la valeur voulue. |
| T41 | FR42 | « ne communiquera pas vos coordonnées sans votre consentement explicite » | « peut communiquer… avec consentement » | La garantie négative devient une permission. |
| T42 | FR44 | « le site fonctionne toujours, rien n'est cassé », puis la classe dynamique supprimée casse le menu | Garder seulement la première affirmation | Le constat de succès est contredit par la suite du texte. Une réécriture ne doit pas inventer de succès. |
| T43 | FR44 | « tous les fichiers à la racine du site (et uniquement ceux-là) » | « les fichiers du site » | La portée change. |
| T44 | FR44 | note mobile 91 → 99 sur goetter.fr | « Critical améliore la note à 99 » | Une mesure unique et locale devient un gain général. |
