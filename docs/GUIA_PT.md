# SocialEye — Guia em Português (Portugal)

O SocialEye é uma aplicação gratuita e de código aberto para Windows 10/11 que reúne a gestão de Instagram, Facebook, YouTube e podcasts no Spotify: conteúdos, publicação, estatísticas e notificações. Os dados ficam no seu computador; não há servidor SocialEye nem telemetria.

## Instalação
1. Transfira o instalador (`.exe` ou `.msi`) da página *Releases* do repositório.
2. Abra o SocialEye e mude o idioma em **Definições → Geral → Idioma** (por omissão segue o idioma do Windows).

## Ligar plataformas
O SocialEye comunica com cada plataforma através da **sua** aplicação de programador:
1. Siga o [guia de configuração](API_SETUP.md) (em inglês) para criar a aplicação no Google, Meta ou Spotify.
2. Em **Definições → Aplicações das plataformas** cole o Client ID (e o segredo, quando necessário). O segredo fica no Gestor de Credenciais do Windows.
3. Registe no fornecedor o URI de redirecionamento indicado: `http://127.0.0.1:53682/callback`.
4. Em **Contas** clique em **Ligar …**. Inicia sessão na página oficial da plataforma — o SocialEye nunca vê a sua palavra-passe.

## Limitações importantes
- **Instagram:** nenhuma API fornece a lista de seguidores. Para saber quem não o segue de volta, peça a exportação dos seus dados (Centro de Contas → As suas informações e permissões → Transferir as suas informações → Seguidores e a seguir, formato JSON) e importe `followers_1.json` e `following.json` em **Instagram → Análise de seguidores**. Os ficheiros são lidos localmente.
- **Instagram:** só contas profissionais (Criador/Empresa) ligadas a uma Página do Facebook. O agendamento é feito pelo SocialEye e só funciona com a aplicação aberta.
- **Facebook:** apenas Páginas; perfis pessoais não são acessíveis pela API.
- **YouTube:** impressões e CTR não estão disponíveis na API de análise. Shorts são identificados por duração (≤ 3 min) e marcados como “prováveis”.
- **Spotify:** reproduções, ouvintes e seguidores só existem no Spotify for Creators. Transfira o CSV de análises e importe-o em **Spotify → Importar CSV**. Não existe API pública de publicação de episódios.

Quando um valor não é fornecido pela plataforma, aparece como **Indisponível** (com fundo tracejado) e nunca como zero.

## Cópias de segurança
**Definições → Dados**: criar e restaurar cópias de segurança (`.db`) e exportar todos os dados em JSON. Os tokens e segredos nunca são incluídos nas exportações.

## Problemas frequentes
Consulte [TROUBLESHOOTING.md](TROUBLESHOOTING.md). Os mais comuns: URI de redirecionamento diferente do registado, porta ocupada (altere-a nas Definições) e contas que precisam de nova ligação após expirar a autorização.
