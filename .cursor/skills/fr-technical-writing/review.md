# Relecture indépendante — corpus français

Second relecteur, distinct du chercheur. Date : 2026-11-07 (heure de Saïgon, UTC+7). Les originaux du chercheur n'ont pas été modifiés.

## Verdict
**30 sources acceptées (passe 2)**. Passe 1 : FR01 et FR29 retirés, FR37 et FR38 ajoutés. Passe 2 : FR11, FR19, FR21 et FR22 retirés, FR39, FR41, FR42 et FR44 ajoutés. Lus intégralement : les 30 textes acceptés, FR01 et FR29, HANDOFF, checklist, manifestes, provenance E01–E17, analysis (R01–R14, T1–T15) et le projet de skill. Pour FR01, la page anglaise équivalente a aussi été lue.

## Retraits
- **FR01** (De Oliveira, février 2017). La version anglaise du même auteur date du même mois. Le texte français contient du lorem anglais (« Good design will be better… »), l'intertitre « CSS is awesome » et des coquilles d'influence anglaise (« utilises », « rhythme »). Le chercheur affirme que le texte cite « l'article anglais », mais cette mention est absente : on lit seulement « j'utiliserais ce terme en anglais ». Le sens de la traduction est donc indémontrable. La requête Wayback CDX est revenue vide (vérification incomplète). C'est une exclusion, pas une accusation.
- **FR29** (Pascal Martin, 2014). Billet commémoratif d'environ 600 mots, pas un article technique substantiel.

## Remplacements
- **FR37** Porteneuve, « Comprendre et maîtriser les clés SSH », publié le 13 septembre 2010.
- **FR38** Porteneuve, « Truthy ou falsy », publié le 10 septembre 2012.
- Reconnaissance : E18 (Eyrolles, livre de 2008 ; la notice de l'éditeur mentionne la présidence de Paris-Web en 2008).
- Exclus de ces deux textes : la promotion de formation en fin de page, les commentaires, et les faits de sécurité de FR37 (DSA 1024, « 256 bits SSL », délais de cassage), qui sont obsolètes et ne font pas autorité.

## Passe 2 (2026-11-07, 14:29–15:00, heure de Saïgon, UTC+7) : écarts de preuve et reconnaissance

Pour cette passe, j'ai fait des lectures supplémentaires. Ce n'est pas une nouvelle passe du chercheur : les remplacements sont mes propres lectures intégrales.

- **E19 retrouvé et enregistré** : https://www.duchess-france.fr/les-conferences/speakeuses/2019/08/05/retour-sur-devoxx-france-2019.html (HTTP 200, `evidence/review2/E19a.html`, sha256 16557fdf…). Extrait : « The Boring Architecture, ou comment construire une licorne sur un monolith par Michel Domenjoud et Nicolas De Nayer ». L'identité avec l'auteur de FR20 n'est établie que par le nom. La page devconf.net trouvée par la recherche n'a pas pu être récupérée : elle n'est pas citée.
- **E20 retrouvé et enregistré** : https://blog.octo.com/hexagonal-architecture-three-principles-and-an-implementation-example (HTTP 200, `evidence/review2/E20.html`, sha256 e1bb09dd…). Extrait : « le 15/10/2018 par Erwan Alliaume , Sébastien Roccaserra » et « Accéder à la traduction en français ». **Correction de la passe 1** : cette page présente le texte français comme une traduction, ce qui contredit l'ordre des dates. La conclusion « français d'abord » pour FR22 est affaiblie, pas confirmée. La question est sans objet puisque FR22 est retiré.
- **Reconnaissance des auteurs qui n'en avaient qu'au niveau de la publication** (recherche bornée, sans inventer de popularité) :
  - Hatat : une thèse de 2013 et une biographie OpenWeb. Ce sont des titres, pas une reconnaissance.
  - Gery et Dessaint : rien.
  - Raoul : seulement des homonymes sans rapport.
  - Roccaserra : ses propres publications et une participation à un podcast en 2023, ce qui reste faible.
  - **Conclusion** : FR11, FR19, FR21 et FR22 sont retirés. Les 5 auteurs ne couvraient que 4 sources, d'où 4 remplacements.
- **Remplacements lus intégralement** (originaux français ; dates et révisions vérifiées sur chaque page) :
  - FR39 : Porteneuve, Git merge et rebase, 2014-05-04. Exclus : les passages sur Git 2.22 et « tou·te·s ».
  - FR41 : Pascal Martin, API de hachage des mots de passe de PHP 5.5, 2012-10-18. Les sources anglaises (RFC, conférence) ne sont que citées.
  - FR42 : Rimelé, Géolocalisation en HTML5, 2011-08-11, mis à jour le 2026-09-27. Les ajouts tardifs sont exclus ; le texte vient d'une conférence KiwiParty donnée en français.
  - FR44 : Goetter, workflow Gulp, 2015-07-16, mis à jour le 2015-12-13. Le tweet anglais est exclu.
- **Lu sans être retenu** : FR43 (Bortzmeyer, 2007), qui contient des modifications ultérieures non signalées. **Rejeté** : FR40, qui relaie une réponse anglaise de StackOverflow.
- **Résultat** : 30 sources, toutes d'auteurs avec une reconnaissance individuelle. Dates du 2003-06-02 au 2019-09-05. 11 auteurs, 7 sites (OpenWeb 9, Alsacréations 6, Bortzmeyer 5, iamvdo 3, P. Martin 3, Delicious Insights 3, OCTO 1). Les thèmes architecture et tests se réduisent à FR20 et FR39 ; l'outillage (FR44) s'ajoute.
- **Ancrages et tests** : FR11, FR19, FR21 et FR22 sont retirés de la skill et des tests (T18 et T31 réancrés). Tests ajoutés : T34–T44. Les erreurs historiques restent signalées (FR41, FR42, FR44).
- Les versions de la passe 1 sont conservées dans `review-history/*.v1.*`.

## Points prioritaires vérifiés
- **FR17.** Écrit en avril 2014 par Crofte, refondu intégralement en mai 2017, signature actuelle Goetter, mise à jour en direct le 2025-04-20. La formulation actuelle est attribuée à la refonte de 2017 par inférence. Le passage W3C traduit est exclu.
- **FR22.** Comparé à Cockburn : la structure et la prose françaises sont originales. Le code de Pierrain est réutilisé avec permission. Les citations de Cockburn et SOLID (anglais ou traduites) sont exclues. La version anglaise OCTO date du 15/10/2018, après le texte français du 17/07/2018. La note de vocabulaire non datée est exclue.
- **FR28.** IBM n'est cité que comme lieu où l'auteur a découvert la classe ; les exemples sont les siens. Le second benchmark compare des bornes inégales (1000 vs 10000) et ne fait pas autorité.
- **FR25/FR26.** L'antériorité du français est confirmée (E12 dit explicitement que la version anglaise est une traduction ; FR26 date de 2014, la version anglaise de 2015).
- **Citations traduites exclues :** FR10, FR12, FR33 (spécifications) et FR11 (tableaux ECMA). L'ajout de FR12 daté de janvier 2005 est antérieur à la date limite.
- **Reconnaissance.** Hatat, Gery, Dessaint, Raoul et Roccaserra ne sont reconnus qu'au niveau de la publication. Domenjoud a une intervention à Devoxx France 2019 (preuve individuelle partielle, URL à revérifier). *(Passe 1, désormais remplacé : voir la section passe 2 ; ces sources ont été retirées.)*

## Diversité (passe 1, historique ; chiffres actuels dans la section passe 2)
- Dates : de 2003-03-21 à 2019-09-05.
- 16 signatures actuelles, 7 sites : OpenWeb 10, Bortzmeyer 5, Alsacréations 4, OCTO 4, iamvdo 3, Pascal Martin 2, Delicious Insights 2.
- Environ 12 textes CSS, mise en page ou images HTML ; les autres portent sur le réseau, JS, PHP, les méthodes et Git/SSH.
- Biais : Web et France.

## Corrections des règles
- Toutes les citations de FR01 et FR29 ont été remplacées.
- R01 : ajout du `nous` communautaire.
- R07 : ajout du présent daté de sécurité (FR37).
- R08 : ajout de la recommandation au conditionnel et de l'hypothèse non transformée en diagnostic.
- R09 : ajout de `ne… pas que`, de nécessairement / pas nécessairement et du défaut appliqué par `||`.
- R14 : ajout des sorties littérales localisées.
- Les extrapolations UI et référence sont marquées explicitement.
- Tests : T2 devient ambigu, T8 est affaibli, ajout de T16–T33 (voir [tests.md](tests.md)).

## Intégration
Ces fichiers sont dans `.cursor/skills/fr-technical-writing/` : [SKILL.md](SKILL.md), [sources.md](sources.md), [sources.json](sources.json), [tests.md](tests.md), [review.md](review.md). Aucun texte intégral. `evidence/review2/`, `review-history/` et les copies d'articles ne sont pas dans le dépôt. Ce n'est pas un plugin de marketplace ni une compétence d'exécution.
