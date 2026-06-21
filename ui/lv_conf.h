/**
 * @file lv_conf.h
 * LVGL 配置 — M5StickS3 (240x135, RGB565)
 * 最小化配置以减小固件体积
 */

#ifndef LV_CONF_H
#define LV_CONF_H

/* 颜色深度 */
#define LV_COLOR_DEPTH 16

/* 屏幕尺寸 */
#define LV_HOR_RES_MAX 240
#define LV_VER_RES_MAX 135

/* 内存 */
#define LV_MEM_CUSTOM 0
#define LV_MEM_SIZE (32 * 1024)   /* 32KB LVGL heap */

/* 启用需要的模块 */
#define LV_USE_BTN 1
#define LV_USE_LABEL 1
#define LV_USE_BAR 1
#define LV_USE_LINE 1

/* 禁用不需要的模块 */
#define LV_USE_ANIMATION 0
#define LV_USE_FS_IF 0
#define LV_USE_GPU 0
#define LV_USE_LOG 0
#define LV_USE_THEME 0

/* 字体 */
#define LV_FONT_MONTSERRAT_12 1
#define LV_FONT_DEFAULT &lv_font_montserrat_12

/* 性能 */
#define LV_DISP_DEF_REFR_PERIOD 30   /* 30ms 刷新 */

#endif /* LV_CONF_H */
