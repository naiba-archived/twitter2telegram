ALTER TABLE `follows` ADD COLUMN `follow_rt_count` BIGINT UNSIGNED NOT NULL DEFAULT 0;
ALTER TABLE `follows` ADD COLUMN `block_rt_count` BIGINT UNSIGNED NOT NULL DEFAULT 0;