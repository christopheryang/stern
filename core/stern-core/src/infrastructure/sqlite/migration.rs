use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(VaultMeta::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(VaultMeta::Id).text().primary_key())
                    .col(ColumnDef::new(VaultMeta::VaultSalt).binary().not_null())
                    .col(ColumnDef::new(VaultMeta::VerifyHash).binary().not_null())
                    .col(
                        ColumnDef::new(VaultMeta::EncryptedIndex)
                            .binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(VaultMeta::KdfParams).text().not_null())
                    .col(ColumnDef::new(VaultMeta::CreatedAt).text().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Entries::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Entries::Id).text().primary_key())
                    .col(ColumnDef::new(Entries::Kind).text().not_null())
                    .col(ColumnDef::new(Entries::Name).text().not_null())
                    .col(ColumnDef::new(Entries::Tags).text())
                    .col(ColumnDef::new(Entries::DekWrapped).binary().not_null())
                    .col(ColumnDef::new(Entries::Nonce).binary().not_null())
                    .col(ColumnDef::new(Entries::Ciphertext).binary().not_null())
                    .col(
                        ColumnDef::new(Entries::Version)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(Entries::CreatedAt).text().not_null())
                    .col(ColumnDef::new(Entries::UpdatedAt).text().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Entries::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(VaultMeta::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum VaultMeta {
    Table,
    Id,
    VaultSalt,
    VerifyHash,
    EncryptedIndex,
    KdfParams,
    CreatedAt,
}

#[derive(Iden)]
enum Entries {
    Table,
    Id,
    Kind,
    Name,
    Tags,
    DekWrapped,
    Nonce,
    Ciphertext,
    Version,
    CreatedAt,
    UpdatedAt,
}
