"""Generates the three dashboards of this folder for Fly's managed Grafana (plan O6).

Run from anywhere: python3 monitoring/fly-grafana/generate.py
"""
import json
import os

OUT = os.path.dirname(os.path.abspath(__file__))
# Reference palette, dark steps (Grafana's default theme), fixed order; status colors reserved
SLOT=['#3987e5','#d95926','#199e70','#c98500','#d55181','#008300','#9085e9','#e66767']
GOOD,CRITICAL='#0ca30c','#d03b3b'
DS={'type':'prometheus','uid':'${datasource}'}
A='app="$app"'

def variables():
    return {'list':[
        {'name':'datasource','label':'Source','type':'datasource','query':'prometheus','current':{},'hide':0},
        {'name':'app','label':'Bot','type':'custom','query':'just-ingredients,just-ingredients-staging',
         'current':{'text':'just-ingredients','value':'just-ingredients'},'hide':0,
         'options':[{'text':'just-ingredients','value':'just-ingredients','selected':True},
                    {'text':'just-ingredients-staging','value':'just-ingredients-staging','selected':False}]},
        {'name':'db_app','label':'Base','type':'custom','query':'just-ingredients-db,just-ingredients-staging-db',
         'current':{'text':'just-ingredients-db','value':'just-ingredients-db'},'hide':0,
         'options':[{'text':'just-ingredients-db','value':'just-ingredients-db','selected':True},
                    {'text':'just-ingredients-staging-db','value':'just-ingredients-staging-db','selected':False}]},
    ]}

class Layout:
    def __init__(self): self.id=0
    def next(self): self.id+=1; return self.id

def stat(L, title, expr, x, y, w=6, h=4, unit='short', decimals=0, description='', time_from=None,
         sparkline=False, mappings=None, background=False):
    p={'id':L.next(),'type':'stat','title':title,'description':description,'datasource':DS,
       'gridPos':{'h':h,'w':w,'x':x,'y':y},
       'targets':[{'refId':'A','datasource':DS,'expr':expr,'instant':not sparkline,'range':sparkline}],
       'fieldConfig':{'defaults':{'unit':unit,'decimals':decimals,'mappings':mappings or [],
            'color':{'mode':'fixed','fixedColor':SLOT[0]},
            'thresholds':{'mode':'absolute','steps':[{'color':'text','value':None}]}},'overrides':[]},
       'options':{'reduceOptions':{'calcs':['lastNotNull'],'fields':'','values':False},
            # Values wear the text color; a status tile wears its status color with its label
            'colorMode':'background' if background else 'none',
            'graphMode':'area' if sparkline else 'none','textMode':'value','justifyMode':'center',
            'orientation':'auto','wideLayout':True,'showPercentChange':False}}
    if time_from:
        p['timeFrom']=time_from
    return p

def series(L, title, targets, x, y, w=12, h=8, unit='short', bars=False, stack=False, names=None,
           description='', interval=None, decimals=None, points=False):
    """targets: list of (expr, legend). names: {series name: (display name, color)} in slot order."""
    custom={'drawStyle':'bars' if bars else 'line','lineWidth':0 if bars else 2,
            'fillOpacity':85 if bars else 0,'gradientMode':'none','showPoints':'always' if points else 'never',
            'pointSize':8,'spanNulls':False,'axisSoftMin':0,'barAlignment':0,
            'stacking':{'mode':'normal' if stack else 'none','group':'A'},
            'lineInterpolation':'linear','axisBorderShow':False,'axisCenteredZero':False}
    overrides=[]
    for name,(display,color) in (names or {}).items():
        overrides.append({'matcher':{'id':'byName','options':name},'properties':[
            {'id':'displayName','value':display},{'id':'color','value':{'mode':'fixed','fixedColor':color}}]})
    defaults={'unit':unit,'custom':custom,'color':{'mode':'fixed','fixedColor':SLOT[0]}}
    if decimals is not None: defaults['decimals']=decimals
    p={'id':L.next(),'type':'timeseries','title':title,'description':description,'datasource':DS,
       'gridPos':{'h':h,'w':w,'x':x,'y':y},
       'targets':[{'refId':chr(65+i),'datasource':DS,'expr':e,'legendFormat':l,'range':True,'instant':False}
                  for i,(e,l) in enumerate(targets)],
       'fieldConfig':{'defaults':defaults,'overrides':overrides},
       # A legend for two series or more; the crosshair tooltip lists every series at the hovered time
       'options':{'legend':{'displayMode':'list','placement':'bottom','showLegend':len(targets)>1 or bool(names) and len(names)>1},
                  'tooltip':{'mode':'multi','sort':'none'}}}
    if interval: p['interval']=interval
    return p

def bargauge(L, title, expr, legend, x, y, w=12, h=11, names=None, description=''):
    overrides=[{'matcher':{'id':'byName','options':n},'properties':[{'id':'displayName','value':d}]}
               for n,d in (names or {}).items()]
    return {'id':L.next(),'type':'bargauge','title':title,'description':description,'datasource':DS,
       'gridPos':{'h':h,'w':w,'x':x,'y':y},
       'targets':[{'refId':'A','datasource':DS,'expr':expr,'legendFormat':legend,'instant':True,'range':False}],
       # One kind of thing: one hue for every bar
       'fieldConfig':{'defaults':{'unit':'short','decimals':0,'min':0,
            'color':{'mode':'fixed','fixedColor':SLOT[0]},
            'thresholds':{'mode':'absolute','steps':[{'color':SLOT[0],'value':None}]}},'overrides':overrides},
       'options':{'orientation':'horizontal','displayMode':'basic','showUnfilled':False,'valueMode':'text',
            'namePlacement':'left','sizing':'auto','minVizHeight':16,'maxVizHeight':24,
            'reduceOptions':{'calcs':['lastNotNull'],'fields':'','values':False}}}

def dashboard(uid, title, description, panels):
    return {'uid':uid,'title':title,'description':description,'tags':['justingredients'],
            'timezone':'Europe/Paris','editable':True,'graphTooltip':1,'refresh':'5m',
            'schemaVersion':39,'version':1,'time':{'from':'now-14d','to':'now'},
            'templating':variables(),'annotations':{'list':[]},'links':[],'panels':panels}

DAY='[1d]'


def per_value(metric, label, values, extra=''):
    """One daily query per known value of `label`, in this order: the stacks and the legend keep
    it (one query grouped `by` the label gives the series in any order). values: [(value,
    display name)]. Returns (targets, names)."""
    targets, names = [], {}
    for i, (value, display) in enumerate(values):
        targets.append((f'sum(increase({metric}{{{A}{extra},{label}="{value}"}}{DAY}))', display))
        names[display] = (display, SLOT[i])
    return targets, names


def per_value_p95(metric, label, values):
    targets, names = [], {}
    for i, (value, display) in enumerate(values):
        targets.append((f'histogram_quantile(0.95, sum by (le) (increase({metric}_bucket{{{A},{label}="{value}"}}{DAY})))', display))
        names[display] = (display, SLOT[i])
    return targets, names

# ---------- Usage ----------
L=Layout()
photos=per_value('ji_photos_total','result',[('read','lue'),('not_recipe','pas une recette'),('unreadable','illisible'),
             ('too_large','trop grande'),('daily_limit','limite du jour'),('gemini_error','erreur Gemini'),
             ('download_failed','téléchargement échoué')])
created=per_value('ji_recipes_created_total','kind',[('photo','photo'),('linked','recette liée')])
action_names={'start':'/start','help':'aide','list':'liste','search':'recherche','card_opened':'fiche ouverte',
              'linked_opened':'recette liée ouverte','rename':'renommage','edit':'modification','delete':'suppression',
              'review_confirmed':'photo validée','ingredient_edited':'ingrédient modifié',
              'ingredient_deleted':'ingrédient supprimé','unit_view_chosen':'vue des unités choisie',
              'note_added':'mémo ajouté','note_skipped':'mémo passé','note_edited':'mémo modifié',
              'note_cleared':'mémo effacé','api_key_created':'clé créée','api_key_revoked':'clé révoquée'}
mcp_requests=per_value('ji_api_requests_total','status',
  [('200','acceptée'),('401','clé refusée'),('429','trop nombreuses'),('503','occupé'),('504','trop longue'),('413','trop grande')],
  extra=',door="mcp"')
mcp_tools={'list_recipes':'lister','search_recipes':'chercher','get_recipe':'lire une recette',
           'convert_quantity':'convertir'}
usage=dashboard('ji-usage','JustIngredients · Usage',
  'Utilisateurs, photos, recettes et actions (plan O, specs/tasks/observability.md). Fly garde 15 jours ; au-delà, /stats et le bilan du lundi.',[
  stat(L,'Utilisateurs actifs 24 h',f'max(ji_users{{{A},period="day"}})',0,0,sparkline=True,
       description='Vus dans les dernières 24 heures (calculé chaque heure).'),
  stat(L,'Utilisateurs actifs 7 jours',f'max(ji_users{{{A},period="week"}})',6,0,sparkline=True),
  stat(L,'Inscrits',f'max(ji_users{{{A},period="total"}})',12,0,sparkline=True,
       description='Toute personne qui a écrit au bot.'),
  stat(L,'Recettes enregistrées',f'max(ji_recipes{{{A}}})',18,0,sparkline=True),
  series(L,'Photos par jour',photos[0],0,4,
         bars=True,stack=True,names=photos[1],interval='1d',decimals=0,
         description='Résultat de chaque photo reçue.'),
  series(L,'Recettes créées par jour',created[0],12,4,
         bars=True,stack=True,names=created[1],interval='1d',decimals=0,
         description='« recette liée » : chaque partie de plus d\'une photo à plusieurs sections.'),
  series(L,'Actions par jour',[(f'sum(increase(ji_actions_total{{{A}}}{DAY}))','actions')],0,12,h=11,
         bars=True,interval='1d',decimals=0),
  bargauge(L,'Actions sur la période',f'sort_desc(sum by (action) (increase(ji_actions_total{{{A}}}[$__range])))',
           '{{action}}',12,12,h=11,names=action_names),
  series(L,'Utilisateurs actifs dans le temps',[(f'max(ji_users{{{A},period="day"}})','24 h'),
         (f'max(ji_users{{{A},period="week"}})','7 jours')],0,23,w=24,decimals=0,
         names={'24 h':('24 h',SLOT[0]),'7 jours':('7 jours',SLOT[1])}),
  series(L,'Assistants IA : requêtes MCP par jour',mcp_requests[0],0,31,w=12,h=8,
         bars=True,stack=True,names=mcp_requests[1],interval='1d',decimals=0,
         description='Requêtes reçues sur /mcp (specs/mcp.md), par réponse : acceptées, clé refusée (401), trop nombreuses (429), occupé (503)…'),
  bargauge(L,'Assistants IA : outils appelés sur la période',
           f'sort_desc(sum by (tool) (increase(ji_mcp_tool_calls_total{{{A}}}[$__range])))',
           '{{tool}}',12,31,h=8,names=mcp_tools),
])
# ---------- Gemini ----------
L=Layout()
COST=f'sum(increase(ji_gemini_cost_micro_usd_total{{{A}}}[$__range])) / 1e6'
R='ji_gemini_requests_total'
calls=([(f'sum(increase({R}{{{A},result="success"}}{DAY}))','succès'),
        (f'sum(increase({R}{{{A},result!~"success|circuit_open"}}{DAY}))','échec'),
        (f'sum(increase({R}{{{A},result="circuit_open"}}{DAY}))','refusé par le disjoncteur')],
       {'succès':('succès',SLOT[0]),'échec':('échec',SLOT[1]),'refusé par le disjoncteur':('refusé par le disjoncteur',SLOT[2])})
tokens=per_value('ji_gemini_tokens_total','kind',[('input','entrée'),('output','sortie')])
gemini=dashboard('ji-gemini','JustIngredients · Gemini et coûts',
  'Appels à Gemini, tokens, coût estimé et durée de lecture d\'une photo (plan O).',[
  stat(L,'Coût estimé aujourd\'hui',COST,0,0,unit='currencyUSD',decimals=2,time_from='now/d',
       description='Depuis minuit (heure de Paris). Alerte Telegram au-delà de 1 $.'),
  stat(L,'Coût estimé sur la période',COST,6,0,unit='currencyUSD',decimals=2),
  stat(L,'Coût par photo lue',f'({COST}) / sum(increase(ji_photos_total{{{A},result="read"}}[$__range]))',12,0,
       unit='currencyUSD',decimals=3),
  stat(L,'Photos servies par le cache',
       f'(sum(increase(ji_gemini_cache_hits_total{{{A}}}[$__range])) or vector(0)) / '
       f'((sum(increase(ji_gemini_cache_hits_total{{{A}}}[$__range])) or vector(0)) + '
       f'sum(increase(ji_gemini_requests_total{{{A},result!="circuit_open"}}[$__range])))',18,0,
       unit='percentunit',decimals=0,description='Une photo déjà lue est resservie sans nouvel appel payant.'),
  series(L,'Appels par jour',calls[0],0,4,
         bars=True,stack=True,names=calls[1],interval='1d',decimals=0,
         description='Les échecs par type (délai, quota, réponse invalide…) : ji_gemini_requests_total dans Explore, ou les logs.'),
  series(L,'Coût estimé par jour',[(f'sum(increase(ji_gemini_cost_micro_usd_total{{{A}}}{DAY})) / 1e6','coût')],12,4,
         bars=True,unit='currencyUSD',interval='1d',decimals=3),
  series(L,'Tokens par jour',tokens[0],0,12,
         bars=True,stack=True,names=tokens[1],interval='1d',decimals=0),
  series(L,'Durée d\'une lecture de photo',[
         (f'histogram_quantile(0.5, sum by (le) (increase(ji_gemini_duration_seconds_bucket{{{A}}}{DAY})))','médiane'),
         (f'histogram_quantile(0.95, sum by (le) (increase(ji_gemini_duration_seconds_bucket{{{A}}}{DAY})))','95 %')],12,12,
         unit='s',interval='1d',points=True,
         names={'médiane':('médiane',SLOT[0]),'95 %':('95 %',SLOT[1])},
         description='Nouvelles tentatives comprises, par jour.'),
])
# ---------- Health ----------
L=Layout()
errors=per_value('ji_errors_total','component',[('gemini','Gemini'),('telegram','Telegram'),('database','base'),('internal','interne')])
db_p95=per_value_p95('ji_db_duration_seconds','operation',[('save_recipe','enregistrement'),('apply_ingredient_changes','modification'),
                     ('create_recipe','création'),('create_ingredient','ingrédient')])
health=dashboard('ji-health','JustIngredients · Santé',
  'Erreurs, disjoncteur de Gemini, redémarrages, rapidité, machines et base (plan O).',[
  stat(L,'Gemini',f'max(ji_gemini_breaker_open{{{A}}})',0,0,background=True,
       mappings=[{'type':'value','options':{'0':{'text':'✔ répond','color':GOOD,'index':0},
                                            '1':{'text':'✖ en panne','color':CRITICAL,'index':1}}}],
       description='En panne : le disjoncteur s\'est ouvert après plusieurs échecs de suite, jusqu\'au prochain appel réussi '
                   '(les photos sont refusées une minute, puis un essai passe).'),
  stat(L,'Dernier démarrage',f'max(ji_started_at_seconds{{{A}}}) * 1000',6,0,unit='dateTimeFromNow'),
  stat(L,'Redémarrages sur la période',f'sum(changes(ji_started_at_seconds{{{A}}}[$__range]))',12,0,
       description='Mises en production comprises.'),
  stat(L,'Erreurs sur la période',f'sum(increase(ji_errors_total{{{A}}}[$__range])) or vector(0)',18,0),
  series(L,'Erreurs par jour',errors[0],0,4,
         bars=True,stack=True,interval='1d',decimals=0,names=errors[1]),
  series(L,'Rapidité, 95 % des cas',[
         (f'histogram_quantile(0.95, sum by (le) (increase(ji_update_duration_seconds_bucket{{{A},kind="message"}}{DAY})))','message'),
         (f'histogram_quantile(0.95, sum by (le) (increase(ji_update_duration_seconds_bucket{{{A},kind="callback"}}{DAY})))','bouton'),
         (f'histogram_quantile(0.95, sum by (le) (increase(ji_card_shown_seconds_bucket{{{A}}}{DAY})))','fiche')],12,4,
         unit='s',interval='1d',points=True,
         names={'message':('message',SLOT[0]),'bouton':('bouton',SLOT[1]),'fiche':('affichage d\'une fiche',SLOT[2])},
         description='Par jour. Un message avec photo comprend la lecture par Gemini.'),
  series(L,'Mémoire utilisée',[
         ('sum(fly_instance_memory_mem_total{app="$app"} - fly_instance_memory_mem_available{app="$app"})','bot'),
         ('sum(fly_instance_memory_mem_total{app="$db_app"} - fly_instance_memory_mem_available{app="$db_app"})','base')],0,12,
         unit='bytes',names={'bot':('bot',SLOT[0]),'base':('base de données',SLOT[1])},
         description='Métriques des machines fournies par Fly.'),
  series(L,'CPU',[
         ('sum(rate(fly_instance_cpu{app="$app",mode!="idle"}[$__rate_interval])) / 100','bot'),
         ('sum(rate(fly_instance_cpu{app="$db_app",mode!="idle"}[$__rate_interval])) / 100','base')],12,12,
         unit='percentunit',names={'bot':('bot',SLOT[0]),'base':('base de données',SLOT[1])},
         description='Part d\'un processeur, métriques fournies par Fly.'),
  stat(L,'Taille de la base',
       'sum(pg_database_size_bytes{app="$db_app",datname!~"template0|template1|postgres|repmgr"})',0,20,w=8,unit='bytes',decimals=1),
  series(L,'Requêtes en base, 95 % des cas',db_p95[0],8,20,w=16,
         unit='s',interval='1d',points=True,names=db_p95[1]),
])
for name,d in [('usage',usage),('gemini',gemini),('health',health)]:
    with open(os.path.join(OUT,f'{name}.json'),'w') as f:
        json.dump(d,f,ensure_ascii=False,indent=2); f.write('\n')
    print(name, len(d['panels']), 'panels')
